//! Putting packs on disk, safely: download, check, unpack aside, check again, then move into
//! `packs/` as one folder. A pack folder is never written in place (running apps keep its files
//! mapped), and a failure at any step leaves nothing in `packs/`.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use typescript_core::pack::{self, Manifest};

use crate::catalog::{Catalog, CatalogPack};
use crate::env::Env;
use crate::error::{err, Error, Result};
use crate::fetch::{resolve, Fetch};
use crate::hash::sha256_hex;

/// Room above the catalog's size for the zip's own headers.
const SLACK: usize = 4096;
/// No table in a pack is anywhere near this: a zip claiming more is a bomb.
const MAX_UNPACKED_FILE: u64 = 256 * 1024 * 1024;
/// A pack folder left in staging this long is the leftover of a crash.
const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Installed {
    New,
    Already,
}

#[derive(Debug, Default)]
pub struct RemoveReport {
    pub removed: Vec<String>,
    /// Folders that could not be deleted (a running app holds them), with the reason.
    pub kept: Vec<(String, String)>,
}

/// Removes staging folders older than `older_than`.
pub fn clean_staging(env: &Env, older_than: Duration) {
    let Ok(entries) = fs::read_dir(env.staging()) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age >= older_than);
        if old {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Removes the staging folder when dropped, whatever happens.
struct Staged(PathBuf);

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The folder holding `id` at `version`, whatever it is called (a folder a running app would
/// not let go of can force the next install into `-r1`).
pub fn installed_folder(env: &Env, id: &str, version: &str) -> Option<PathBuf> {
    pack::scan(&env.packs)
        .packs
        .into_iter()
        .find(|f| f.manifest.id == id && f.manifest.version == version)
        .map(|f| f.dir)
}

/// Where a new pack goes: its usual folder, or the first free `-rN` when something is stuck there.
fn free_folder(env: &Env, pack: &CatalogPack) -> Result<PathBuf> {
    let usual = env.pack_folder(&pack.id, &pack.version);
    if !usual.exists() {
        return Ok(usual);
    }
    for n in 1..=20 {
        let next = env.packs.join(format!("{}-{}-r{n}", pack.id, pack.version));
        if !next.exists() {
            return Ok(next);
        }
    }
    err(format!(
        "{}: too many leftover folders in {}",
        pack.id,
        env.packs.display()
    ))
}

pub fn install_pack(
    env: &Env,
    fetch: &dyn Fetch,
    base: &str,
    pack: &CatalogPack,
) -> Result<Installed> {
    if installed_folder(env, &pack.id, &pack.version).is_some() {
        return Ok(Installed::Already);
    }
    clean_staging(env, STALE_AFTER);

    let max = (pack.size as usize).saturating_add(SLACK);
    let bytes = fetch.get(&resolve(base, &pack.url), max)?;
    if bytes.len() as u64 != pack.size {
        return err(format!("{}: the download has the wrong size", pack.id));
    }
    if !sha256_hex(&bytes).eq_ignore_ascii_case(&pack.sha256) {
        return err(format!("{}: checksum mismatch, not installing", pack.id));
    }

    let stage = env.staging().join(format!(
        "{}-{}-{}",
        pack.id,
        pack.version,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage)?;
    let staged = Staged(stage.clone());
    unpack(&bytes, &stage)?;
    verify(&stage, pack)?;

    fs::create_dir_all(&env.packs)?;
    let target = free_folder(env, pack)?;
    match fs::rename(&stage, &target) {
        Ok(()) => {
            std::mem::forget(staged); // moved: nothing left to clean
            Ok(Installed::New)
        }
        // Someone installed the same pack between our check and now: theirs is as good.
        Err(_) if installed_folder(env, &pack.id, &pack.version).is_some() => {
            Ok(Installed::Already)
        }
        Err(e) => err(format!("{}: could not move into place: {e}", pack.id)),
    }
}

/// A pack is a handful of files.
const MAX_ENTRIES: usize = 64;

/// Names Windows treats specially: a drive or stream (`a:b`), a device (`CON`, `aux.txt`,
/// `COM1`), a trailing dot or space.
fn ordinary_name(name: &str) -> bool {
    if name.contains(':') || name.ends_with('.') || name.ends_with(' ') {
        return false;
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let numbered = (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.len() == 4
        && stem.as_bytes()[3].is_ascii_digit();
    !(matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered)
}

/// Flat files only: a pack is a manifest and its tables, and a name that is not a plain file
/// name (a path, `..`, a drive) is an attack, not a pack.
pub(crate) fn unpack(bytes: &[u8], into: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > MAX_ENTRIES {
        return err("the pack has too many files");
    }
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let plain = Path::new(&name)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == name && n != ".." && n != ".");
        if !plain || entry.enclosed_name().is_none() || !ordinary_name(&name) {
            return err(format!("the pack contains an unsafe path: {name}"));
        }
        let mut out = fs::File::create(into.join(&name))?;
        let copied = std::io::copy(&mut (&mut entry).take(MAX_UNPACKED_FILE + 1), &mut out)?;
        if copied > MAX_UNPACKED_FILE {
            return err(format!("{name} is too large"));
        }
    }
    Ok(())
}

/// The unpacked folder is the pack the catalog promised: same id and version, and every table
/// the manifest lists is there with the hash the manifest gives.
fn verify(dir: &Path, pack: &CatalogPack) -> Result<()> {
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(dir.join("manifest.json"))
            .map_err(|_| Error(format!("{}: the pack has no manifest.json", pack.id)))?,
    )?;
    if manifest.id != pack.id || manifest.version != pack.version {
        return err(format!(
            "{}: the pack is {} {}, not what the release lists",
            pack.id, manifest.id, manifest.version
        ));
    }
    for (name, hash) in &manifest.files {
        let bytes = fs::read(dir.join(name))
            .map_err(|_| Error(format!("{}: {name} is missing", pack.id)))?;
        if !sha256_hex(&bytes).eq_ignore_ascii_case(hash) {
            return err(format!("{}: {name} does not match its manifest", pack.id));
        }
    }
    Ok(())
}

/// Installs `id` and the packs it needs; each outcome in install order.
pub fn install_with_requirements(
    env: &Env,
    fetch: &dyn Fetch,
    base: &str,
    catalog: &Catalog,
    id: &str,
) -> Result<Vec<(String, Installed)>> {
    let mut done = Vec::new();
    for pack in catalog.with_requirements(id)? {
        done.push((pack.id.clone(), install_pack(env, fetch, base, pack)?));
    }
    Ok(done)
}

/// Deletes every installed version of `id`.
pub fn remove_pack(env: &Env, id: &str) -> Result<RemoveReport> {
    let mut report = RemoveReport::default();
    let Ok(entries) = fs::read_dir(&env.packs) else {
        return Ok(report);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_ours = path.is_dir()
            && name.starts_with(&format!("{id}-"))
            && fs::read(path.join("manifest.json"))
                .ok()
                .and_then(|d| serde_json::from_slice::<Manifest>(&d).ok())
                .is_some_and(|m| m.id == id);
        if !is_ours {
            continue;
        }
        match remove_pack_folder(&path) {
            Ok(()) => report.removed.push(name),
            Err(e) => report.kept.push((name, e.to_string())),
        }
    }
    Ok(report)
}

/// Deletes a pack folder, the tables first and manifest.json last: a table an app still has
/// mapped can't be deleted, and then the folder stays a whole pack (with its manifest) instead
/// of a manifest-less stump that blocks the next install.
pub(crate) fn remove_pack_folder(path: &Path) -> std::io::Result<()> {
    let mut stuck: Option<std::io::Error> = None;
    for entry in fs::read_dir(path)?.flatten() {
        if entry.file_name() == "manifest.json" {
            continue;
        }
        let gone = if entry.path().is_file() {
            fs::remove_file(entry.path())
        } else {
            Err(std::io::Error::other("unexpected folder inside a pack"))
        };
        if let Err(e) = gone {
            stuck.get_or_insert(e);
        }
    }
    if let Some(e) = stuck {
        return Err(e);
    }
    fs::remove_file(path.join("manifest.json"))?;
    fs::remove_dir(path)
}

/// Ids of the installed packs that need `id`.
pub fn required_by(env: &Env, id: &str) -> Vec<String> {
    let mut ids: Vec<String> = pack::scan(&env.packs)
        .packs
        .into_iter()
        .filter(|f| f.manifest.requires.iter().any(|r| r == id))
        .map(|f| f.manifest.id)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Dir;
    use crate::testutil::{fixture, pack_zip, zip_of};

    fn base() -> &'static str {
        "https://example.test/release/packs.json"
    }

    #[test]
    fn installs_a_pack_the_text_service_can_find() {
        let f = fixture("install");
        let en = f.catalog.pack("en").unwrap();
        assert_eq!(
            install_pack(&f.env, &f.source, base(), en).unwrap(),
            Installed::New
        );
        let found = typescript_core::pack::scan(&f.env.packs);
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        assert_eq!(found.packs.len(), 1);
        assert_eq!(found.packs[0].manifest.id, "en");
        assert!(f.env.packs.join("en-0.4.0").join("table.txt").is_file());
        assert!(
            !f.env.staging().exists() || std::fs::read_dir(f.env.staging()).unwrap().count() == 0
        );
    }

    #[test]
    fn a_pack_that_is_already_there_is_left_alone() {
        let f = fixture("again");
        let en = f.catalog.pack("en").unwrap();
        install_pack(&f.env, &f.source, base(), en).unwrap();
        let marker = f.env.packs.join("en-0.4.0").join("table.txt");
        let before = std::fs::metadata(&marker).unwrap().modified().unwrap();
        assert_eq!(
            install_pack(&f.env, &f.source, base(), en).unwrap(),
            Installed::Already
        );
        assert_eq!(
            std::fs::metadata(&marker).unwrap().modified().unwrap(),
            before
        );
    }

    #[test]
    fn a_pack_with_the_wrong_checksum_is_refused_and_leaves_nothing() {
        let f = fixture("badsum");
        let mut en = f.catalog.pack("en").unwrap().clone();
        en.sha256 = "0".repeat(64);
        let e = install_pack(&f.env, &f.source, base(), &en).unwrap_err();
        assert!(e.0.contains("checksum"), "{e}");
        assert!(!f.env.pack_folder("en", "0.4.0").exists());
    }

    #[test]
    fn a_pack_of_the_wrong_size_is_refused() {
        let f = fixture("badsize");
        let mut en = f.catalog.pack("en").unwrap().clone();
        en.size += 1;
        assert!(install_pack(&f.env, &f.source, base(), &en).is_err());
    }

    #[test]
    fn a_zip_that_escapes_its_folder_is_refused() {
        let f = fixture("escape");
        let zip = zip_of(&[("manifest.json", b"{}"), ("../evil.txt", b"x")]);
        std::fs::write(f.root.join("release").join("evil.zip"), &zip).unwrap();
        let pack = crate::catalog::CatalogPack {
            id: "evil".into(),
            name: "evil".into(),
            version: "1".into(),
            requires: vec![],
            size: zip.len() as u64,
            sha256: crate::hash::sha256_hex(&zip),
            url: "evil.zip".into(),
        };
        assert!(install_pack(&f.env, &f.source, base(), &pack).is_err());
        assert!(!f
            .root
            .join("local")
            .join("Typescript")
            .join("evil.txt")
            .exists());
        assert!(!f.root.join("evil.txt").exists());
        assert!(!f.env.pack_folder("evil", "1").exists());
    }

    #[test]
    fn a_table_that_does_not_match_the_manifest_is_refused() {
        let f = fixture("badtable");
        // The manifest promises one table content; the zip carries another.
        let good = pack_zip("zz", "1", &[], &[("table.txt", b"right")]);
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(good)).unwrap();
        let mut manifest = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("manifest.json").unwrap(),
            &mut manifest,
        )
        .unwrap();
        let zip = zip_of(&[
            ("manifest.json", manifest.as_bytes()),
            ("table.txt", b"tampered"),
        ]);
        std::fs::write(f.root.join("release").join("zz.zip"), &zip).unwrap();
        let pack = crate::catalog::CatalogPack {
            id: "zz".into(),
            name: "zz".into(),
            version: "1".into(),
            requires: vec![],
            size: zip.len() as u64,
            sha256: crate::hash::sha256_hex(&zip),
            url: "zz.zip".into(),
        };
        let e = install_pack(&f.env, &f.source, base(), &pack).unwrap_err();
        assert!(e.0.contains("table.txt"), "{e}");
        assert!(!f.env.pack_folder("zz", "1").exists());
    }

    #[test]
    fn a_pack_that_is_not_what_the_catalog_says_is_refused() {
        let f = fixture("wrongid");
        let mut ko = f.catalog.pack("ko").unwrap().clone();
        ko.url = f.catalog.pack("en").unwrap().url.clone(); // en's zip served as ko
        ko.size = f.catalog.pack("en").unwrap().size;
        ko.sha256 = f.catalog.pack("en").unwrap().sha256.clone();
        assert!(install_pack(&f.env, &f.source, base(), &ko).is_err());
        assert!(!f.env.pack_folder("ko", "0.4.0").exists());
    }

    #[test]
    fn installing_a_language_brings_the_packs_it_needs() {
        let f = fixture("requires");
        let done = install_with_requirements(&f.env, &f.source, base(), &f.catalog, "ko").unwrap();
        assert_eq!(
            done,
            vec![
                ("en".to_string(), Installed::New),
                ("ko".to_string(), Installed::New)
            ]
        );
        assert!(f.env.pack_folder("en", "0.4.0").is_dir());
    }

    #[test]
    fn removing_deletes_every_version_and_reports_what_it_could_not() {
        let f = fixture("inst-remove");
        let ko = f.catalog.pack("ko").unwrap();
        install_pack(&f.env, &f.source, base(), ko).unwrap();
        // An older version of the same pack from an earlier release.
        let old = f.env.pack_folder("ko", "0.3.9");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("manifest.json"),
            r#"{"id":"ko","name":"ko","version":"0.3.9","core":">=0.3.0","files":{}}"#,
        )
        .unwrap();
        let report = remove_pack(&f.env, "ko").unwrap();
        assert_eq!(report.removed.len(), 2);
        assert!(report.kept.is_empty());
        assert!(!f.env.pack_folder("ko", "0.4.0").exists());
        // Removing what is not there is not an error.
        assert!(remove_pack(&f.env, "ko").unwrap().removed.is_empty());
    }

    #[test]
    fn a_pack_other_packs_need_is_reported() {
        let f = fixture("neededby");
        install_with_requirements(&f.env, &f.source, base(), &f.catalog, "ko").unwrap();
        assert_eq!(required_by(&f.env, "en"), vec!["ko".to_string()]);
        assert!(required_by(&f.env, "ko").is_empty());
    }

    #[test]
    fn leftovers_in_staging_are_cleaned() {
        let f = fixture("stale");
        let stale = f.env.staging().join("old-123");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("half.bin"), b"x").unwrap();
        clean_staging(&f.env, std::time::Duration::ZERO);
        assert!(!stale.exists());
    }

    #[test]
    fn a_missing_download_is_an_error_with_nothing_left() {
        let f = fixture("missing");
        let nowhere = Dir(f.root.join("nowhere"));
        let en = f.catalog.pack("en").unwrap();
        assert!(install_pack(&f.env, &nowhere, base(), en).is_err());
        assert!(!f.env.pack_folder("en", "0.4.0").exists());
    }

    #[test]
    fn a_folder_a_running_app_left_behind_does_not_block_a_reinstall() {
        let f = fixture("inst-leftover");
        // What a failed delete leaves: the folder is there, its manifest is gone.
        let stuck = f.env.pack_folder("ko", "0.4.0");
        std::fs::create_dir_all(&stuck).unwrap();
        std::fs::write(stuck.join("table.txt"), b"mapped by an app").unwrap();
        let ko = f.catalog.pack("ko").unwrap();
        assert_eq!(
            install_pack(&f.env, &f.source, base(), ko).unwrap(),
            Installed::New
        );
        let found = typescript_core::pack::scan(&f.env.packs);
        assert!(
            found.packs.iter().any(|p| p.manifest.id == "ko"),
            "{:?}",
            found.problems
        );
        assert_eq!(
            install_pack(&f.env, &f.source, base(), ko).unwrap(),
            Installed::Already
        );
    }

    #[test]
    fn deleting_keeps_the_manifest_until_the_tables_are_gone() {
        let f = fixture("inst-order");
        let ko = f.catalog.pack("ko").unwrap();
        install_pack(&f.env, &f.source, base(), ko).unwrap();
        let folder = f.env.pack_folder("ko", "0.4.0");
        // A table that cannot be deleted (a directory standing in for a locked file).
        std::fs::remove_file(folder.join("table.txt")).unwrap();
        std::fs::create_dir(folder.join("table.txt")).unwrap();
        std::fs::write(folder.join("table.txt").join("inner"), b"x").unwrap();
        let report = remove_pack(&f.env, "ko").unwrap();
        assert!(
            folder.join("manifest.json").is_file(),
            "the manifest goes last"
        );
        assert!(report.removed.is_empty() && !report.kept.is_empty());
    }

    #[test]
    fn names_windows_treats_specially_are_refused() {
        let f = fixture("inst-names");
        for name in ["CON", "aux.txt", "a:b", "stream.txt:hidden", "NUL", "com1"] {
            let zip = zip_of(&[("manifest.json", b"{}"), (name, b"x")]);
            std::fs::write(f.root.join("release").join("n.zip"), &zip).unwrap();
            let pack = crate::catalog::CatalogPack {
                id: "nn".into(),
                name: "nn".into(),
                version: "1".into(),
                requires: vec![],
                size: zip.len() as u64,
                sha256: crate::hash::sha256_hex(&zip),
                url: "n.zip".into(),
            };
            assert!(
                install_pack(&f.env, &f.source, base(), &pack).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn the_program_packages_of_every_system_unpack() {
        let linux = [
            "typescript",
            "typescript-config",
            "ibus-engine-typescript",
            "typescript.xml.in",
            "install.sh",
            "VERSION",
            "THIRD_PARTY_NOTICES.txt",
        ];
        let mac = [
            "typescript",
            "typescript-config",
            "Typescript.app.tar",
            "install.sh",
            "VERSION",
        ];
        for (os, names) in [("linux", &linux[..]), ("macos", &mac[..])] {
            let files: Vec<(&str, &[u8])> = names.iter().map(|n| (*n, &b"x"[..])).collect();
            let dir = std::env::temp_dir().join(format!("ts-unpack-{os}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            unpack(&zip_of(&files), &dir).unwrap_or_else(|e| panic!("{os}: {e}"));
            assert!(names.iter().all(|n| dir.join(n).is_file()), "{os}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
