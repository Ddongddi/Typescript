//! Is there a newer release, and bringing the installed packs (and the program) up to it.
//! Packs update without administrator rights; the program is replaced by running the release's
//! own install script (install.ps1 asks for administrator rights; install.sh installs for the
//! current user).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::catalog::{Catalog, CORE_VERSION};
use crate::env::Env;
use crate::error::{err, Error, Result};
use crate::fetch::Fetch;
use crate::install::{install_pack, unpack};
use crate::source::Source;
use typescript_core::pack;

pub const LATEST_API: &str =
    "https://api.github.com/repos/iamallyouneed/Typescript/releases/latest";
const DAY: u64 = 24 * 60 * 60;
const API_MAX: usize = 2 * 1024 * 1024;
/// The program package of this system's release and the script in it that installs it.
#[cfg(windows)]
pub const INSTALLER_ZIP: &str = "typescript-windows.zip";
#[cfg(windows)]
pub const INSTALLER_SCRIPT: &str = "install.ps1";
#[cfg(target_os = "macos")]
pub const INSTALLER_ZIP: &str = "typescript-macos.zip";
#[cfg(all(not(windows), not(target_os = "macos")))]
pub const INSTALLER_ZIP: &str = "typescript-linux.zip";
#[cfg(not(windows))]
pub const INSTALLER_SCRIPT: &str = "install.sh";
const INSTALLER_MAX: usize = 200 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub notes: String,
    /// (name, download url)
    pub assets: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateInfo {
    pub available: bool,
    pub latest: String,
    pub notes: String,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The release lookup: GitHub, unless `TYPESCRIPT_RELEASE_API` names a URL or a folder holding
/// a `latest` file (testing an update by hand).
pub fn api_from_env(quick: bool) -> (Box<dyn Fetch>, String) {
    let http = || -> Box<dyn Fetch> {
        Box::new(if quick {
            crate::fetch::Http::quick()
        } else {
            crate::fetch::Http::default()
        })
    };
    match std::env::var("TYPESCRIPT_RELEASE_API") {
        Ok(spec) if spec.contains("://") => (http(), spec),
        Ok(spec) if !spec.is_empty() => (
            Box::new(crate::fetch::Dir(spec.into())),
            "latest".to_string(),
        ),
        _ => (http(), LATEST_API.to_string()),
    }
}

fn parse(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse::<u32>().ok());
    let v = (it.next()??, it.next()??, it.next().unwrap_or(Some(0))?);
    it.next().is_none().then_some(v)
}

/// `a` is a version after `b` (anything that is not a version is never newer).
pub fn is_newer(a: &str, b: &str) -> bool {
    matches!((parse(a), parse(b)), (Some(a), Some(b)) if a > b)
}

fn read_object(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap_or_default();
    let bytes = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
        .unwrap_or(&bytes);
    serde_json::from_slice(bytes).unwrap_or_else(|_| json!({}))
}

/// `checkUpdates` in settings.json (on unless it says false).
pub fn check_enabled(env: &Env) -> bool {
    read_object(&env.settings)["checkUpdates"]
        .as_bool()
        .unwrap_or(true)
}

fn cache_path(env: &Env) -> std::path::PathBuf {
    env.settings.with_file_name("update.json")
}

fn info(latest: &str, notes: &str) -> UpdateInfo {
    UpdateInfo {
        available: is_newer(latest, CORE_VERSION),
        latest: latest.trim_start_matches('v').to_string(),
        notes: notes.to_string(),
    }
}

pub fn fetch_release(fetch: &dyn Fetch, url: &str) -> Result<Release> {
    let bytes = fetch.get(url, API_MAX)?;
    let v: Value = serde_json::from_slice(&bytes)?;
    let Some(tag) = v["tag_name"].as_str() else {
        return err("the release has no version");
    };
    let assets = v["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    Some((
                        a["name"].as_str()?.to_string(),
                        a["browser_download_url"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release {
        tag: tag.to_string(),
        notes: v["body"].as_str().unwrap_or("").to_string(),
        assets,
    })
}

/// Looks for a newer release. Unless `force`, it stays quiet when checking is turned off and
/// reuses an answer less than a day old (so no command waits on the network every time).
pub fn check(env: &Env, fetch: &dyn Fetch, url: &str, now: u64, force: bool) -> Result<UpdateInfo> {
    if !force {
        if !check_enabled(env) {
            return Ok(UpdateInfo {
                available: false,
                latest: String::new(),
                notes: String::new(),
            });
        }
        let cached = read_object(&cache_path(env));
        let when = cached["checkedAt"].as_u64().unwrap_or(0);
        if when != 0 && now >= when && now - when < DAY {
            return Ok(info(
                cached["latest"].as_str().unwrap_or(""),
                cached["notes"].as_str().unwrap_or(""),
            ));
        }
    }
    let release = fetch_release(fetch, url)?;
    let remembered = json!({ "checkedAt": now, "latest": release.tag, "notes": release.notes });
    if let Some(dir) = cache_path(env).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(cache_path(env), remembered.to_string());
    Ok(info(&release.tag, &release.notes))
}

/// Deletes the other versions of `id`; one a running app still holds stays for next time.
pub(crate) fn prune_old(env: &Env, id: &str, keep: &str) {
    let Ok(entries) = std::fs::read_dir(&env.packs) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let manifest: Option<pack::Manifest> = std::fs::read(path.join("manifest.json"))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok());
        if manifest.is_some_and(|m| m.id == id && is_newer(keep, &m.version)) {
            let _ = crate::install::remove_pack_folder(&path);
        }
    }
}

/// Ids of the installed packs the catalog has a newer version of.
pub fn outdated_packs(env: &Env, catalog: &Catalog) -> Vec<String> {
    let mut ids: Vec<String> = pack::scan(&env.packs)
        .packs
        .into_iter()
        .filter(|f| {
            catalog
                .pack(&f.manifest.id)
                .is_some_and(|p| is_newer(&p.version, &f.manifest.version))
        })
        .map(|f| f.manifest.id)
        .collect();
    ids.sort();
    ids
}

/// Installs the catalog's version of every installed pack that has a newer one, then removes the
/// older folders. Packs that were never installed are not touched. Returns the ids updated.
pub fn update_packs(
    env: &Env,
    fetch: &dyn Fetch,
    base: &str,
    catalog: &Catalog,
) -> Result<Vec<String>> {
    let mut updated = Vec::new();
    for found in pack::scan(&env.packs).packs {
        let Some(listed) = catalog.pack(&found.manifest.id) else {
            continue;
        };
        if !is_newer(&listed.version, &found.manifest.version) {
            continue;
        }
        install_pack(env, fetch, base, listed)?;
        prune_old(env, &listed.id, &listed.version);
        updated.push(listed.id.clone());
    }
    updated.sort();
    Ok(updated)
}

/// The hosts GitHub serves release files from.
pub fn trusted_download(url: &str) -> bool {
    [
        "https://github.com/",
        "https://objects.githubusercontent.com/",
        "https://api.github.com/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}

fn asset<'a>(release: &'a Release, name: &str) -> Result<&'a str> {
    match release.assets.iter().find(|(n, _)| n == name) {
        Some((_, url)) => Ok(url),
        None => err(format!("the release has no {name}")),
    }
}

/// Downloads the release's installer package, checks its SHA-256, unpacks it and hands its
/// install script to `launch`. Nothing is started unless the checksum matches.
///
/// The checksum comes from the same release as the program, so it catches a damaged download,
/// not a bad release. What the program is trusted for is where it comes from: over the network
/// only github.com hosts are accepted.
pub fn run_installer(
    env: &Env,
    fetch: &dyn Fetch,
    release: &Release,
    launch: &dyn Fn(&Path) -> Result<()>,
) -> Result<()> {
    let zip_url = asset(release, INSTALLER_ZIP)?;
    let sum_url = asset(release, &format!("{INSTALLER_ZIP}.sha256"))?;
    if fetch.is_remote() && !(trusted_download(zip_url) && trusted_download(sum_url)) {
        return err("the program is offered from a host other than GitHub, not installing");
    }
    let zip = fetch.get(zip_url, INSTALLER_MAX)?;
    let sum = String::from_utf8_lossy(&fetch.get(sum_url, 4096)?).into_owned();
    let expected = sum.split_whitespace().next().unwrap_or("");
    if !crate::hash::sha256_hex(&zip).eq_ignore_ascii_case(expected) {
        return err("the downloaded program does not match its checksum, not installing");
    }
    let stage = env
        .staging()
        .join(format!("program-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage)?;
    unpack(&zip, &stage)?;
    let script = stage.join(INSTALLER_SCRIPT);
    if !script.is_file() {
        return err(format!("the program package has no {INSTALLER_SCRIPT}"));
    }
    launch(&script)
}

/// The installer: the elevated PowerShell script in its own window on Windows, `sh install.sh`
/// (which installs for the current user and asks for nothing) elsewhere.
pub fn launch_installer(script: &Path) -> Result<()> {
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        c
    } else {
        std::process::Command::new("sh")
    };
    command
        .arg(script)
        .spawn()
        .map(|_| ())
        .map_err(|e| Error(format!("could not start the installer: {e}")))
}

/// Everything an update means: the packs first (no administrator needed), then the program if
/// the release has a newer one. Returns what happened, for the user.
pub fn apply(
    env: &Env,
    source: &Source,
    api: &dyn Fetch,
    api_url: &str,
    launch: &dyn Fn(&Path) -> Result<()>,
) -> Result<String> {
    let release = fetch_release(api, api_url)?;
    let program_is_newer = is_newer(&release.tag, CORE_VERSION);
    let mut said = Vec::new();
    // A release for another program version has packs this program can't read: those wait
    // until the program is updated.
    match source.catalog() {
        Ok(catalog) => {
            let updated = update_packs(env, source.fetch.as_ref(), &source.base, &catalog)?;
            if !updated.is_empty() {
                said.push(format!("language packs updated: {}", updated.join(", ")));
            }
        }
        Err(e) if !program_is_newer => return Err(e),
        Err(_) => {}
    }
    if program_is_newer {
        run_installer(env, api, &release, launch)?;
        said.push(format!(
            "started the installer for {}{}; when it is done, close this and run typescript-config again (an open settings page is still the old version)",
            release.tag,
            if cfg!(windows) {
                " (confirm the administrator prompt)"
            } else {
                ""
            }
        ));
    }
    if said.is_empty() {
        said.push("already up to date".to_string());
    }
    Ok(said.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::Fetch;
    use crate::testutil::{fixture, pack_zip};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Serves files from a folder and counts the requests.
    struct Counting {
        inner: crate::fetch::Dir,
        calls: AtomicUsize,
    }

    impl Fetch for Counting {
        fn get(&self, url: &str, max: usize) -> crate::error::Result<Vec<u8>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.get(url, max)
        }
    }

    fn release_json(tag: &str, assets: &[(&str, &str)]) -> String {
        let list: Vec<_> = assets
            .iter()
            .map(|(n, u)| serde_json::json!({ "name": n, "browser_download_url": u }))
            .collect();
        serde_json::json!({ "tag_name": tag, "body": "notes here", "assets": list }).to_string()
    }

    fn api(f: &crate::testutil::Fixture, tag: &str) -> Counting {
        std::fs::write(
            f.root.join("release").join("latest"),
            release_json(tag, &[]),
        )
        .unwrap();
        Counting {
            inner: crate::fetch::Dir(f.root.join("release")),
            calls: AtomicUsize::new(0),
        }
    }

    const URL: &str = "https://api.example.test/repos/x/y/releases/latest";
    const DAY: u64 = 24 * 60 * 60;

    #[test]
    fn versions_compare_by_number_not_text() {
        assert!(is_newer("0.4.1", "0.4.0"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.9.0"));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("0.3.9", "0.4.0"));
        assert!(!is_newer("junk", "0.4.0"));
    }

    #[test]
    fn a_newer_release_is_reported_with_its_notes() {
        let f = fixture("upd-new");
        let fetch = api(&f, "v9.0.0");
        let info = check(&f.env, &fetch, URL, 1000, true).unwrap();
        assert!(info.available);
        assert_eq!(info.latest, "9.0.0");
        assert_eq!(info.notes, "notes here");
    }

    #[test]
    fn the_same_or_an_older_release_is_not_an_update() {
        let f = fixture("upd-same");
        let current = format!("v{}", crate::catalog::CORE_VERSION);
        let fetch = api(&f, &current);
        assert!(!check(&f.env, &fetch, URL, 1000, true).unwrap().available);
    }

    #[test]
    fn the_answer_is_remembered_for_a_day() {
        let f = fixture("upd-cache");
        let fetch = api(&f, "v9.0.0");
        check(&f.env, &fetch, URL, 1000, false).unwrap();
        let again = check(&f.env, &fetch, URL, 1000 + DAY - 1, false).unwrap();
        assert!(again.available, "the cached answer still says so");
        assert_eq!(fetch.calls.load(Ordering::SeqCst), 1);
        check(&f.env, &fetch, URL, 1000 + DAY + 1, false).unwrap();
        assert_eq!(fetch.calls.load(Ordering::SeqCst), 2);
        check(&f.env, &fetch, URL, 1000 + DAY + 2, true).unwrap();
        assert_eq!(
            fetch.calls.load(Ordering::SeqCst),
            3,
            "asking by hand always looks"
        );
    }

    #[test]
    fn checking_can_be_turned_off_but_a_hand_check_still_works() {
        let f = fixture("upd-off");
        std::fs::write(&f.env.settings, r#"{ "checkUpdates": false }"#).unwrap();
        let fetch = api(&f, "v9.0.0");
        assert!(!check(&f.env, &fetch, URL, 1000, false).unwrap().available);
        assert_eq!(fetch.calls.load(Ordering::SeqCst), 0);
        assert!(check(&f.env, &fetch, URL, 1000, true).unwrap().available);
    }

    #[test]
    fn no_network_is_an_error_not_a_crash() {
        let f = fixture("upd-net");
        let nowhere = crate::fetch::Dir(f.root.join("nowhere"));
        assert!(check(&f.env, &nowhere, URL, 1000, true).is_err());
    }

    #[test]
    fn installed_packs_move_to_the_newer_version_and_the_old_one_goes() {
        let f = fixture("upd-packs");
        // ko 0.3.0 is installed; the release has 0.4.0.
        let old = f.env.pack_folder("ko", "0.3.0");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("manifest.json"),
            r#"{"id":"ko","name":"ko","version":"0.3.0","core":">=0.4.0, <99.0.0","files":{}}"#,
        )
        .unwrap();
        let updated =
            update_packs(&f.env, &f.source, "https://x.test/packs.json", &f.catalog).unwrap();
        assert_eq!(updated, vec!["ko".to_string()]);
        assert!(f.env.pack_folder("ko", "0.4.0").is_dir());
        assert!(!old.exists());
        assert!(
            !f.env.pack_folder("ja", "0.4.0").exists(),
            "packs nobody installed stay uninstalled"
        );
        let again =
            update_packs(&f.env, &f.source, "https://x.test/packs.json", &f.catalog).unwrap();
        assert!(again.is_empty());
        let _ = pack_zip;
    }

    #[test]
    fn outdated_packs_are_the_installed_ones_with_a_newer_version_listed() {
        let f = fixture("upd-outdated");
        let old = f.env.pack_folder("ko", "0.3.0");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(
            old.join("manifest.json"),
            r#"{"id":"ko","name":"ko","version":"0.3.0","core":">=0.4.0, <99.0.0","files":{}}"#,
        )
        .unwrap();
        assert_eq!(outdated_packs(&f.env, &f.catalog), vec!["ko".to_string()]);
        update_packs(&f.env, &f.source, "https://x.test/packs.json", &f.catalog).unwrap();
        assert!(outdated_packs(&f.env, &f.catalog).is_empty());
    }

    #[test]
    fn the_program_update_is_checked_before_it_is_started() {
        let f = fixture("upd-bin");
        let zip = crate::testutil::zip_of(&[(INSTALLER_SCRIPT, b"# installer")]);
        std::fs::write(f.root.join("release").join(INSTALLER_ZIP), &zip).unwrap();
        let sum = crate::hash::sha256_hex(&zip);
        std::fs::write(
            f.root
                .join("release")
                .join(format!("{INSTALLER_ZIP}.sha256")),
            format!("{sum}  {INSTALLER_ZIP}"),
        )
        .unwrap();
        let release = Release {
            tag: "v9.0.0".into(),
            notes: String::new(),
            assets: vec![
                (
                    INSTALLER_ZIP.into(),
                    format!("https://x.test/{INSTALLER_ZIP}"),
                ),
                (
                    format!("{INSTALLER_ZIP}.sha256"),
                    format!("https://x.test/{INSTALLER_ZIP}.sha256"),
                ),
            ],
        };
        let started = std::cell::RefCell::new(None);
        run_installer(&f.env, &f.source, &release, &|script| {
            *started.borrow_mut() = Some(script.to_path_buf());
            Ok(())
        })
        .unwrap();
        let script = started.borrow().clone().unwrap();
        assert!(script.ends_with(INSTALLER_SCRIPT) && script.is_file());

        // A download that does not match its checksum never runs.
        std::fs::write(
            f.root
                .join("release")
                .join(format!("{INSTALLER_ZIP}.sha256")),
            format!("{}  x", "0".repeat(64)),
        )
        .unwrap();
        let never = std::cell::Cell::new(false);
        let refused = run_installer(&f.env, &f.source, &release, &|_| {
            never.set(true);
            Ok(())
        });
        assert!(refused.is_err());
        assert!(!never.get());
    }

    #[test]
    fn pruning_removes_older_versions_only() {
        let f = fixture("upd-prune");
        for v in ["0.3.0", "0.4.0", "0.5.0"] {
            let dir = f.env.pack_folder("ko", v);
            std::fs::create_dir_all(&dir).unwrap();
            let manifest = format!(
                r#"{{"id":"ko","name":"ko","version":"{v}","core":">=0.1.0","files":{{}}}}"#
            );
            std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        }
        prune_old(&f.env, "ko", "0.4.0");
        assert!(!f.env.pack_folder("ko", "0.3.0").exists());
        assert!(f.env.pack_folder("ko", "0.4.0").exists());
        assert!(
            f.env.pack_folder("ko", "0.5.0").exists(),
            "a newer version is not ours to delete"
        );
    }

    #[test]
    fn only_github_hosts_may_serve_the_program() {
        assert!(trusted_download(
            "https://github.com/o/r/releases/download/v1/typescript-windows.zip"
        ));
        assert!(trusted_download(
            "https://objects.githubusercontent.com/x/y"
        ));
        assert!(!trusted_download(
            "https://evil.test/typescript-windows.zip"
        ));
        assert!(!trusted_download("https://github.com.evil.test/x"));
        assert!(!trusted_download("http://github.com/x"));
    }
}
