//! Builds the language packs: the tables `crates/core/build.rs` used to bake into the DLL,
//! written as files with a manifest. `crates/core/build.rs` uses this for the packs the tests
//! and the embedded builds carry; `build-packs` writes the release packs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use sha2::{Digest, Sha256};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct PackSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub requires: &'static [&'static str],
}

/// Korean and Japanese use the English word list too (mixed words, previews, protecting
/// English words from kana), so they need its pack even with English output turned off.
pub const PACKS: &[PackSpec] = &[
    PackSpec {
        id: "en",
        name: "English",
        requires: &[],
    },
    PackSpec {
        id: "ko",
        name: "한국어",
        requires: &["en"],
    },
    PackSpec {
        id: "ja",
        name: "日本語",
        requires: &["en"],
    },
];

/// The core versions a pack built now works with: this minor version.
pub fn core_range() -> String {
    let mut parts = VERSION.split('.');
    let major = parts.next().unwrap_or("0");
    let minor: u32 = parts.next().and_then(|m| m.parse().ok()).unwrap_or(0);
    format!(">={major}.{minor}.0, <{major}.{}.0", minor + 1)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

type Files = Vec<(&'static str, Vec<u8>)>;

/// Writes pack `id` into `dir` (created if needed): its tables and manifest.json.
pub fn write_pack(id: &str, data: &Path, dir: &Path) -> io::Result<()> {
    let spec = PACKS
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("no pack {id}")))?;
    let files: Files = match id {
        "en" => english(data)?,
        "ko" => korean(data)?,
        "ja" => vec![
            (
                "kana_trigrams.txt",
                fs::read(data.join("kana_trigrams.txt"))?,
            ),
            (
                "ipadic_compact.bin",
                fs::read(data.join("ipadic_compact.bin"))?,
            ),
        ],
        _ => unreachable!("checked against PACKS"),
    };
    fs::create_dir_all(dir)?;
    let mut hashes = serde_json::Map::new();
    for (name, bytes) in &files {
        fs::write(dir.join(name), bytes)?;
        hashes.insert(name.to_string(), sha256_hex(bytes).into());
    }
    let manifest = serde_json::json!({
        "id": spec.id,
        "name": spec.name,
        "version": VERSION,
        "core": core_range(),
        "requires": spec.requires,
        "files": hashes,
    });
    let json = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    fs::write(dir.join("manifest.json"), json)
}

const EXTRA_LEVEL: i32 = 35;
const SYMBOLS: usize = 27;

const SHORT_WORDS: &str = "a i ad ah am an as at aw ax be by do eh go ha he hi hm if in is it lo ma me mo my no of oh ok on or ow ox pa \
pc so to tv uh um up us we ya ye yo id ex vs ai ui ux os js ts";

/// Everyday/technical words SCOWL is missing or ranks rare.
const COMMON_WORDS: &str = "ok okay hi hey yeah yep nope lol omg btw thx thanks pls please email emails app apps api apis url urls \
http https www github git commit push pull merge branch repo json yaml xml html css js ts swift python java \
kotlin rust golang npm yarn pnpm node react vue svelte ios macos android iphone ipad mac wifi bluetooth usb \
login logout signup username password online offline website blog vlog youtube google slack notion figma \
debug deploy refactor config backend frontend database cache server client async await todo todos ai llm gpt \
claude chatgpt laptop smartphone selfie emoji internet podcast startup dev devs devops ui ux \
wanna gonna gotta kinda sorta outta lemme gimme dunno yeah yup nah ain cuz tho thru hiya kiddo sayin \
goin doin nothin somethin y'all ya yo hey oi aye \
wouldn couldn shouldn didn doesn isn wasn weren hasn haven hadn needn mustn aren mightn \
makeup brooklyn shakespeare einstein techno pesto americano cosmo promo garde kanji kana romaji ramen";

fn is_ascii_letters(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| c.is_ascii_alphabetic())
}

/// [^, ^, letters..., $] with ^ and $ both encoded as 0.
fn encode(word: &str) -> Vec<usize> {
    let mut codes = vec![0, 0];
    codes.extend(
        word.bytes()
            .filter(u8::is_ascii_lowercase)
            .map(|b| (b - b'a' + 1) as usize),
    );
    codes.push(0);
    codes
}

fn english(data: &Path) -> io::Result<Files> {
    let read = |name: &str| fs::read_to_string(data.join(name));
    let extras: Vec<&str> = COMMON_WORDS.split_whitespace().collect();
    let short: BTreeSet<&str> = SHORT_WORDS.split_whitespace().collect();

    // English levels
    let mut raw: BTreeMap<String, i32> = BTreeMap::new();
    for line in read("english_words.txt")?.lines() {
        let mut it = line.split('\t');
        if let (Some(w), Some(l), None) = (it.next(), it.next(), it.next()) {
            if let Ok(level) = l.trim().parse() {
                raw.insert(w.to_string(), level);
            }
        }
    }
    for w in &extras {
        let w = w.to_lowercase();
        let level = raw.get(&w).copied().unwrap_or(EXTRA_LEVEL).min(EXTRA_LEVEL);
        raw.insert(w, level);
    }
    let mut levels: BTreeMap<String, i32> = BTreeMap::new();
    for (word, level) in raw {
        let word: String = word.chars().filter(|c| *c != '\'').collect();
        if !is_ascii_letters(&word) {
            continue;
        }
        // Many 1–2 letter list entries aren't words people type ("ra", "ko"); only real ones count.
        if word.chars().count() > 2 || short.contains(word.as_str()) {
            let e = levels.entry(word).or_insert(level);
            *e = (*e).min(level);
        }
    }
    // Third column: how often the word is said (FrequencyWords subtitles, 0 = not in it); it
    // orders autocomplete suggestions.
    let mut frequency: BTreeMap<String, u64> = BTreeMap::new();
    for line in read("english_frequency.txt")?.lines() {
        let mut it = line.split_whitespace();
        if let (Some(w), Some(n)) = (it.next(), it.next().and_then(|n| n.parse::<u64>().ok())) {
            let w: String = w.chars().filter(|c| *c != '\'').collect();
            let e = frequency.entry(w).or_default();
            *e = (*e).max(n);
        }
    }
    let text: String = levels
        .iter()
        .map(|(w, l)| format!("{w}\t{l}\t{}\n", frequency.get(w).copied().unwrap_or(0)))
        .collect();
    let levels_file = text.into_bytes();

    // Letter trigrams, trained on Webster's 2nd (public domain) plus the extra words.
    let n = SYMBOLS;
    let mut trigrams = vec![0f32; n * n * n];
    let mut contexts = vec![0f32; n * n];
    let web2 = read("web2.txt")?;
    let spelling = web2
        .lines()
        .map(|l| l.to_lowercase())
        .chain(extras.iter().map(|s| s.to_string()));
    for word in spelling {
        if !is_ascii_letters(&word) {
            continue;
        }
        let codes = encode(&word);
        for i in 2..codes.len() {
            trigrams[(codes[i - 2] * n + codes[i - 1]) * n + codes[i]] += 1.0;
            contexts[codes[i - 2] * n + codes[i - 1]] += 1.0;
        }
    }
    let bytes: Vec<u8> = trigrams
        .iter()
        .chain(contexts.iter())
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let trigrams_file = bytes;

    Ok(vec![
        ("english_levels.txt", levels_file),
        ("trigrams.bin", trigrams_file),
    ])
}

fn korean(data: &Path) -> io::Result<Files> {
    let read = |name: &str| fs::read_to_string(data.join(name));
    // Korean word lists
    // "word<TAB>count" (the count orders autocomplete suggestions); contains() looks at the word only.
    let mut words: BTreeMap<String, u64> = BTreeMap::new();
    for line in read("korean_words.txt")?.lines() {
        let mut fields = line.split('\t');
        let word: &str = fields.next().unwrap_or("");
        if !word.is_empty() {
            let count = fields
                .next()
                .and_then(|n| n.trim().parse::<u64>().ok())
                .unwrap_or(1);
            *words.entry(word.to_string()).or_default() += count;
        }
    }
    let sorted: String = words.iter().map(|(w, n)| format!("{w}\t{n}\n")).collect();
    let words_file = sorted.into_bytes();
    // "word<TAB>rank": the file is in order of how often the words are said, 1 = most.
    let mut vocab: BTreeMap<String, usize> = BTreeMap::new();
    for (i, word) in read("korean_vocabulary.txt")?
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .enumerate()
    {
        vocab.entry(word.to_string()).or_insert(i + 1);
    }
    let ranked: String = vocab.iter().map(|(w, r)| format!("{w}\t{r}\n")).collect();
    let vocab_file = ranked.into_bytes();
    Ok(vec![
        (
            "korean_syllables.txt",
            fs::read(data.join("korean_syllables.txt"))?,
        ),
        ("korean_words_sorted.txt", words_file),
        ("korean_vocab_sorted.txt", vocab_file),
    ])
}

/// Zips the files of a pack folder flat, manifest.json first. Fixed timestamps, so the same
/// input gives the same bytes.
fn zip_flat(dir: &Path) -> io::Result<Vec<u8>> {
    use std::io::Write;
    let mut names: Vec<String> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "manifest.json")
        .collect();
    names.sort();
    names.insert(0, "manifest.json".to_string());
    let mut out = io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut out);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for name in &names {
            writer
                .start_file(name.as_str(), options)
                .map_err(io::Error::other)?;
            writer.write_all(&fs::read(dir.join(name))?)?;
        }
        writer.finish().map_err(io::Error::other)?;
    }
    Ok(out.into_inner())
}

/// A release's language-pack assets in `out`: `typescript-pack-<id>-<version>.zip` for every
/// pack, and `packs.json` listing them with their size and SHA-256 (what typescript-config reads).
pub fn write_release(data: &Path, out: &Path) -> io::Result<()> {
    fs::create_dir_all(out)?;
    let work = out.join("work");
    let mut listed = Vec::new();
    for spec in PACKS {
        let folder = work.join(spec.id);
        write_pack(spec.id, data, &folder)?;
        let zip = zip_flat(&folder)?;
        let name = format!("typescript-pack-{}-{VERSION}.zip", spec.id);
        fs::write(out.join(&name), &zip)?;
        listed.push(serde_json::json!({
            "id": spec.id,
            "name": spec.name,
            "version": VERSION,
            "requires": spec.requires,
            "size": zip.len(),
            "sha256": sha256_hex(&zip),
            "url": name,
        }));
    }
    fs::remove_dir_all(&work)?;
    let catalog = serde_json::json!({ "version": VERSION, "core": core_range(), "packs": listed });
    let json = serde_json::to_vec_pretty(&catalog).map_err(io::Error::other)?;
    fs::write(out.join("packs.json"), json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn data() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/data")
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ts-packgen-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn manifest(dir: &Path) -> serde_json::Value {
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap()
    }

    #[test]
    fn writes_a_manifest_with_hashes_of_every_file() {
        let dir = temp("en");
        write_pack("en", &data(), &dir).unwrap();
        let m = manifest(&dir);
        assert_eq!(m["id"], "en");
        assert_eq!(m["version"], VERSION);
        assert_eq!(m["core"], core_range());
        let files = m["files"].as_object().unwrap();
        assert_eq!(files.len(), 2);
        for (name, hash) in files {
            let bytes = fs::read(dir.join(name)).unwrap();
            assert_eq!(hash.as_str().unwrap(), sha256_hex(&bytes), "{name}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn korean_and_japanese_require_english() {
        for id in ["ko", "ja"] {
            let dir = temp(id);
            write_pack(id, &data(), &dir).unwrap();
            assert_eq!(
                manifest(&dir)["requires"],
                serde_json::json!(["en"]),
                "{id}"
            );
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn same_input_same_bytes() {
        let (a, b) = (temp("det-a"), temp("det-b"));
        write_pack("ko", &data(), &a).unwrap();
        write_pack("ko", &data(), &b).unwrap();
        for f in [
            "korean_words_sorted.txt",
            "korean_vocab_sorted.txt",
            "manifest.json",
        ] {
            assert_eq!(fs::read(a.join(f)).unwrap(), fs::read(b.join(f)).unwrap());
        }
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn core_range_covers_this_minor_version() {
        let mut parts = VERSION.split('.');
        let (major, minor) = (parts.next().unwrap(), parts.next().unwrap());
        let next: u32 = minor.parse::<u32>().unwrap() + 1;
        assert_eq!(
            core_range(),
            format!(">={major}.{minor}.0, <{major}.{next}.0")
        );
    }

    #[test]
    fn unknown_pack_is_an_error() {
        assert!(write_pack("xx", &data(), &temp("xx")).is_err());
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;
    use std::io::Read;
    use std::path::PathBuf;

    fn data() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/data")
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ts-release-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_release_has_one_zip_per_pack_and_a_catalog_that_tells_the_truth() {
        let out = temp("catalog");
        write_release(&data(), &out).unwrap();
        let catalog: serde_json::Value =
            serde_json::from_slice(&fs::read(out.join("packs.json")).unwrap()).unwrap();
        assert_eq!(catalog["version"], VERSION);
        assert_eq!(catalog["core"], core_range());
        let packs = catalog["packs"].as_array().unwrap();
        assert_eq!(packs.len(), PACKS.len());
        for p in packs {
            let url = p["url"].as_str().unwrap();
            let zip = fs::read(out.join(url)).unwrap();
            assert_eq!(p["size"].as_u64().unwrap(), zip.len() as u64, "{url}");
            assert_eq!(p["sha256"].as_str().unwrap(), sha256_hex(&zip), "{url}");
            assert!(url.starts_with("typescript-pack-") && url.ends_with(".zip"));
        }
        let ko = packs.iter().find(|p| p["id"] == "ko").unwrap();
        assert_eq!(ko["requires"], serde_json::json!(["en"]));
        let _ = fs::remove_dir_all(&out);
    }

    #[test]
    fn a_pack_zip_is_flat_with_its_manifest_first() {
        let out = temp("flat");
        write_release(&data(), &out).unwrap();
        let zip = fs::read(out.join(format!("typescript-pack-en-{VERSION}.zip"))).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(names[0], "manifest.json");
        assert!(
            names.iter().all(|n| !n.contains('/') && !n.contains('\\')),
            "{names:?}"
        );
        let mut manifest = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        assert!(manifest.contains("\"id\": \"en\""), "{manifest}");
        let _ = fs::remove_dir_all(&out);
    }
}
