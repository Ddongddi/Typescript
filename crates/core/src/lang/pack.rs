//! Language packs: `<root>/<id>-<version>/manifest.json` plus the tables it lists. Packs are
//! memory-mapped read-only, so the processes sharing them share the pages, as they did when the
//! tables were baked into the DLL. Hashes are checked when a pack is installed (typescript-config),
//! not here: hashing 16 MB in every application that loads the text service would be slow.
//!
//! Missing data never panics: `bytes`/`text` return empty data and `available` says the
//! language can't be used.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use serde::Deserialize;

use crate::decide::engine::Language;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub core: String,
    #[serde(default)]
    pub requires: Vec<String>,
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Found {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub packs: Vec<Found>,
    /// Pack folders that can't be used, with the reason (shown by the dashboard).
    pub problems: Vec<(PathBuf, String)>,
}

fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().split('.').map(|p| p.parse::<u32>().ok());
    let v = (it.next()??, it.next()??, it.next().unwrap_or(Some(0))?);
    it.next().is_none().then_some(v)
}

/// `">=0.4.0, <0.5.0"`: every comma-separated `>=`, `>`, `<=`, `<` or `=` bound holds.
pub fn version_matches(range: &str, version: &str) -> bool {
    let Some(v) = parse_version(version) else {
        return false;
    };
    range.split(',').map(str::trim).all(|bound| {
        let parsed = [">=", "<=", ">", "<", "="].iter().find_map(|op| {
            bound
                .strip_prefix(op)
                .map(|rest| (*op, parse_version(rest)))
        });
        match parsed {
            Some((">=", Some(x))) => v >= x,
            Some(("<=", Some(x))) => v <= x,
            Some((">", Some(x))) => v > x,
            Some(("<", Some(x))) => v < x,
            Some(("=", Some(x))) => v == x,
            _ => false,
        }
    })
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let data = fs::read(dir.join("manifest.json")).map_err(|e| format!("manifest.json: {e}"))?;
    let manifest: Manifest =
        serde_json::from_slice(&data).map_err(|e| format!("manifest.json: {e}"))?;
    if !version_matches(&manifest.core, env!("CARGO_PKG_VERSION")) {
        return Err(format!(
            "needs core {}, this is {}",
            manifest.core,
            env!("CARGO_PKG_VERSION")
        ));
    }
    if let Some(missing) = manifest.files.keys().find(|f| !dir.join(f).is_file()) {
        return Err(format!("{missing} is missing"));
    }
    Ok(manifest)
}

/// Every usable pack under `root`: per id, the newest version this core accepts.
pub fn scan(root: &Path) -> Scan {
    let mut out = Scan::default();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    let mut best: HashMap<String, Found> = HashMap::new();
    for dir in entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        match read_manifest(&dir) {
            Ok(manifest) => {
                let newer = best.get(&manifest.id).is_none_or(|f| {
                    parse_version(&manifest.version) > parse_version(&f.manifest.version)
                });
                if newer {
                    best.insert(manifest.id.clone(), Found { manifest, dir });
                }
            }
            Err(reason) => out.problems.push((dir, reason)),
        }
    }
    out.packs = best.into_values().collect();
    out.packs.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    out
}

/// Pack data for this process. Entries are only ever added: models keep `&'static` references
/// into them for the life of the process.
#[derive(Default)]
pub(crate) struct Registry {
    files: HashMap<(Language, String), &'static [u8]>,
    requires: HashMap<Language, Vec<Language>>,
}

impl Registry {
    pub(crate) fn insert(
        &mut self,
        language: Language,
        requires: Vec<Language>,
        files: Vec<(String, &'static [u8])>,
    ) {
        if self.requires.contains_key(&language) {
            return; // already loaded: its data stays what the models were built from
        }
        self.requires.insert(language, requires);
        for (name, bytes) in files {
            self.files.insert((language, name), bytes);
        }
    }

    pub(crate) fn add_scan(&mut self, scan: Scan) {
        for found in scan.packs {
            let Some(language) = Language::from_id(&found.manifest.id) else {
                continue;
            };
            if self.requires.contains_key(&language) {
                continue;
            }
            let files: Option<Vec<(String, &'static [u8])>> = found
                .manifest
                .files
                .keys()
                .map(|name| map(&found.dir.join(name)).map(|bytes| (name.clone(), bytes)))
                .collect();
            if let Some(files) = files.filter(|f| usable(language, f)) {
                let requires = found
                    .manifest
                    .requires
                    .iter()
                    .filter_map(|id| Language::from_id(id))
                    .collect();
                self.insert(language, requires, files);
            }
        }
    }

    pub(crate) fn has(&self, language: Language) -> bool {
        self.requires
            .get(&language)
            .is_some_and(|req| req.iter().all(|r| self.requires.contains_key(r)))
    }

    pub(crate) fn bytes(&self, language: Language, name: &str) -> &'static [u8] {
        self.files
            .get(&(language, name.to_string()))
            .copied()
            .unwrap_or(&[])
    }
}

/// A pack whose data can't be used is left out like a missing one (the Japanese dictionary is
/// checked because the converter would panic on a damaged file).
fn usable(language: Language, files: &[(String, &'static [u8])]) -> bool {
    language != Language::Japanese
        || files
            .iter()
            .find(|(name, _)| name == "ipadic_compact.bin")
            .is_some_and(|(_, bytes)| crate::decide::kanji::KanjiDictionary::valid(bytes))
}

/// Maps a file for the rest of the process.
/// ponytail: the mapping is leaked on purpose (models hold `&'static` slices into it); a new
/// pack version maps a second copy, bounded by how many updates one process lives through.
fn map(path: &Path) -> Option<&'static [u8]> {
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() == 0 {
        return Some(&[]);
    }
    // SAFETY: pack files are never modified in place: an update writes a new version folder
    // (install.ps1 skips folders that exist, typescript-config will do the same), and Windows
    // refuses to truncate or overwrite a file that has a mapped view, so the mapped bytes
    // can't change under the models.
    let mmap = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    let leaked: &'static memmap2::Mmap = Box::leak(Box::new(mmap));
    Some(&leaked[..])
}

#[allow(unused_mut, unused_macros)] // without any embedded-* feature nothing is inserted
fn embedded() -> Registry {
    let mut registry = Registry::default();
    macro_rules! pack_file {
        ($id:literal, $name:literal) => {
            (
                $name.to_string(),
                include_bytes!(concat!(env!("OUT_DIR"), "/packs/", $id, "/", $name))
                    as &'static [u8],
            )
        };
    }
    #[cfg(feature = "embedded-en")]
    registry.insert(
        Language::English,
        vec![],
        vec![
            pack_file!("en", "english_levels.txt"),
            pack_file!("en", "trigrams.bin"),
        ],
    );
    #[cfg(feature = "embedded-ko")]
    registry.insert(
        Language::Korean,
        vec![Language::English],
        vec![
            pack_file!("ko", "korean_syllables.txt"),
            pack_file!("ko", "korean_words_sorted.txt"),
            pack_file!("ko", "korean_vocab_sorted.txt"),
        ],
    );
    #[cfg(feature = "embedded-ja")]
    registry.insert(
        Language::Japanese,
        vec![Language::English],
        vec![
            pack_file!("ja", "kana_trigrams.txt"),
            pack_file!("ja", "ipadic_compact.bin"),
        ],
    );
    registry
}

fn global() -> &'static RwLock<Registry> {
    static GLOBAL: OnceLock<RwLock<Registry>> = OnceLock::new();
    GLOBAL.get_or_init(|| RwLock::new(embedded()))
}

/// A model built from pack data. Until the pack is available it hands out an empty stand-in
/// that is not remembered as the model, so a pack installed later is picked up instead of the
/// process staying empty until it restarts.
pub(crate) struct PackLazy<T> {
    language: Language,
    make: fn() -> T,
    loaded: OnceLock<T>,
    stand_in: OnceLock<T>,
}

impl<T> PackLazy<T> {
    pub(crate) const fn new(language: Language, make: fn() -> T) -> Self {
        Self {
            language,
            make,
            loaded: OnceLock::new(),
            stand_in: OnceLock::new(),
        }
    }

    pub(crate) fn get(&self) -> &T {
        self.get_if(available(self.language))
    }

    /// `available`: the language's pack is loaded (the tests pass it in).
    pub(crate) fn get_if(&self, available: bool) -> &T {
        if let Some(model) = self.loaded.get() {
            return model;
        }
        if available {
            self.loaded.get_or_init(self.make)
        } else {
            self.stand_in.get_or_init(self.make)
        }
    }
}

/// Loads the packs installed under `root` that this process doesn't have yet.
pub fn init_from_dir(root: &Path) {
    let found = scan(root);
    global()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .add_scan(found);
}

/// Picks up packs installed since the last call (the settings changed: a language may be new).
pub fn refresh(root: &Path) {
    init_from_dir(root);
}

/// The language's pack, and every pack it needs, are loaded.
pub fn available(language: Language) -> bool {
    global()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .has(language)
}

pub fn bytes(language: Language, name: &str) -> &'static [u8] {
    global()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .bytes(language, name)
}

pub fn text(language: Language, name: &str) -> &'static str {
    std::str::from_utf8(bytes(language, name)).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ts-pack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, id: &str, version: &str, core: &str) {
        let pack = dir.join(format!("{id}-{version}"));
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("a.txt"), "hello\n").unwrap();
        let manifest = serde_json::json!({
            "id": id, "name": id, "version": version, "core": core,
            "requires": [], "files": { "a.txt": "x" },
        });
        std::fs::write(pack.join("manifest.json"), manifest.to_string()).unwrap();
    }

    fn any_core() -> String {
        format!(">={}, <99.0.0", env!("CARGO_PKG_VERSION"))
    }

    #[test]
    fn version_ranges() {
        assert!(version_matches(">=0.4.0, <0.5.0", "0.4.0"));
        assert!(version_matches(">=0.4.0, <0.5.0", "0.4.9"));
        assert!(!version_matches(">=0.4.0, <0.5.0", "0.5.0"));
        assert!(!version_matches(">=0.4.0, <0.5.0", "0.3.9"));
        assert!(!version_matches("garbage", "0.4.0"));
        assert!(!version_matches(">=0.4.0", "not-a-version"));
    }

    #[test]
    fn scan_picks_the_newest_compatible_version() {
        let dir = temp("newest");
        write(&dir, "en", "0.4.0", &any_core());
        write(&dir, "en", "0.4.2", &any_core());
        write(&dir, "en", "0.9.0", ">=99.0.0, <100.0.0"); // needs a newer core
        let found = scan(&dir);
        assert_eq!(found.packs.len(), 1);
        assert_eq!(found.packs[0].manifest.version, "0.4.2");
        assert_eq!(found.problems.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_packs_are_reported_not_fatal() {
        let dir = temp("broken");
        std::fs::create_dir_all(dir.join("ko-0.4.0")).unwrap(); // no manifest
        std::fs::create_dir_all(dir.join("ja-0.4.0")).unwrap();
        std::fs::write(dir.join("ja-0.4.0/manifest.json"), "{ not json").unwrap();
        let found = scan(&dir);
        assert!(found.packs.is_empty());
        assert_eq!(found.problems.len(), 2);
        assert!(scan(&dir.join("missing")).packs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pack_whose_file_is_missing_is_a_problem() {
        let dir = temp("nofile");
        write(&dir, "en", "0.4.0", &any_core());
        std::fs::remove_file(dir.join("en-0.4.0/a.txt")).unwrap();
        let found = scan(&dir);
        assert!(found.packs.is_empty());
        assert_eq!(found.problems.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_data_reads_as_empty() {
        assert!(bytes(Language::English, "no-such-file.bin").is_empty());
        assert_eq!(text(Language::English, "no-such-file.txt"), "");
    }

    #[test]
    fn refresh_adds_new_packs_and_keeps_loaded_ones() {
        let mut registry = Registry::default();
        let dir = temp("refresh");
        write(&dir, "en", "0.4.0", &any_core());
        registry.add_scan(scan(&dir));
        let first = registry.bytes(Language::English, "a.txt");
        assert_eq!(first, b"hello\n");
        // A newer English and a Korean pack appear while the process runs.
        write(&dir, "en", "0.4.1", &any_core());
        write(&dir, "ko", "0.4.0", &any_core());
        registry.add_scan(scan(&dir));
        assert!(registry.has(Language::Korean));
        // The English data already handed out stays the same (and valid).
        assert_eq!(
            registry.bytes(Language::English, "a.txt").as_ptr(),
            first.as_ptr()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pack_without_its_requirement_is_not_available() {
        let mut registry = Registry::default();
        registry.insert(Language::Korean, vec![Language::English], vec![]);
        assert!(!registry.has(Language::Korean));
        registry.insert(Language::English, vec![], vec![]);
        assert!(registry.has(Language::Korean));
    }

    #[test]
    fn a_japanese_pack_with_a_damaged_dictionary_is_not_registered() {
        let dir = temp("badja");
        write(&dir, "en", "0.4.0", &any_core());
        let pack = dir.join("ja-0.4.0");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("kana_trigrams.txt"), "a\n").unwrap();
        std::fs::write(pack.join("ipadic_compact.bin"), b"TSIPDIC2 cut short").unwrap();
        let manifest = serde_json::json!({
            "id": "ja", "name": "ja", "version": "0.4.0", "core": any_core(),
            "requires": ["en"],
            "files": { "kana_trigrams.txt": "x", "ipadic_compact.bin": "x" },
        });
        std::fs::write(pack.join("manifest.json"), manifest.to_string()).unwrap();
        let mut registry = Registry::default();
        registry.add_scan(scan(&dir));
        assert!(registry.has(Language::English));
        assert!(
            !registry.has(Language::Japanese),
            "a damaged dictionary must not load"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_model_waits_for_its_pack_instead_of_freezing_empty() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static BUILT: AtomicUsize = AtomicUsize::new(0);
        static LAZY: PackLazy<usize> = PackLazy::new(Language::English, || {
            BUILT.fetch_add(1, Ordering::SeqCst) + 1
        });
        // No pack yet: an empty stand-in, built once, and not remembered as the model.
        let stand_in = *LAZY.get_if(false);
        assert_eq!(*LAZY.get_if(false), stand_in);
        assert_eq!(BUILT.load(Ordering::SeqCst), 1);
        // The pack arrives: the real model is built now, from the pack's data.
        let real = *LAZY.get_if(true);
        assert_ne!(real, stand_in);
        assert_eq!(BUILT.load(Ordering::SeqCst), 2);
        // From here on that is the model, whatever availability says.
        assert_eq!(*LAZY.get_if(false), real);
        assert_eq!(BUILT.load(Ordering::SeqCst), 2);
    }
}
