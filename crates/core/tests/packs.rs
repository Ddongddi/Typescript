//! The packs `build-packs` writes are what the text service loads: scanning them finds every
//! language, with nothing wrong, and their manifests tell the truth about the files.

use std::path::PathBuf;

use tacit_core::pack;
use tacit_packgen::{sha256_hex, write_pack, PACKS};

fn built_packs(name: &str) -> PathBuf {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    let root = std::env::temp_dir().join(format!("ts-real-packs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for p in PACKS {
        write_pack(p.id, &data, &root.join(format!("{}-0.0", p.id))).unwrap();
    }
    root
}

#[test]
fn every_released_pack_scans_clean() {
    let root = built_packs("scan");
    let found = pack::scan(&root);
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    let ids: Vec<&str> = found.packs.iter().map(|f| f.manifest.id.as_str()).collect();
    assert_eq!(ids, vec!["en", "ja", "ko"]);
    for f in &found.packs {
        for (name, hash) in &f.manifest.files {
            let bytes = std::fs::read(f.dir.join(name)).unwrap();
            assert_eq!(&sha256_hex(&bytes), hash, "{}/{name}", f.manifest.id);
            assert!(!bytes.is_empty(), "{}/{name} is empty", f.manifest.id);
        }
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_flipped_byte_no_longer_matches_its_hash() {
    let root = built_packs("flip");
    let found = pack::scan(&root);
    let en = found.packs.iter().find(|f| f.manifest.id == "en").unwrap();
    let path = en.dir.join("trigrams.bin");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[0] ^= 0xff;
    assert_ne!(
        sha256_hex(&bytes),
        en.manifest.files["trigrams.bin"],
        "a damaged table must be detectable from the manifest"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_released_dictionary_passes_its_own_validity_check() {
    let root = built_packs("kanji");
    let found = pack::scan(&root);
    let ja = found.packs.iter().find(|f| f.manifest.id == "ja").unwrap();
    let bytes = std::fs::read(ja.dir.join("ipadic_compact.bin")).unwrap();
    assert!(tacit_core::KanjiDictionary::valid(&bytes));
    assert!(!tacit_core::KanjiDictionary::valid(
        &bytes[..bytes.len() - 100]
    ));
    assert!(!tacit_core::KanjiDictionary::valid(b""));
    let _ = std::fs::remove_dir_all(&root);
}
