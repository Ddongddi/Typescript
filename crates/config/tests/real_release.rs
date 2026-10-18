#![cfg(feature = "japanese")]

//! The tool against a release built from the real data: what packgen publishes is what
//! tacit-config installs, and what the text service then loads.
use std::path::PathBuf;

use tacit_config::{cli, env::Env, source::Source};
use tacit_core::{pack, Languages};

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(String::from).collect()
}

#[test]
fn a_real_release_installs_and_loads() {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/data");
    let root = std::env::temp_dir().join(format!("ts-real-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let release = root.join("release");
    tacit_packgen::write_release(&data, &release).unwrap();

    let env = Env {
        settings: root.join("home/settings.json"),
        apps: root.join("home/apps.json"),
        packs: root.join("local/packs"),
    };
    let source = Source::from_spec(release.to_str().unwrap());
    let out = cli::run(&args("setup --languages ko,ja"), &env, &source).unwrap();
    assert!(out.contains("ko, ja"), "{out}");

    let found = pack::scan(&env.packs);
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    let ids: Vec<&str> = found.packs.iter().map(|f| f.manifest.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["en", "ja", "ko"],
        "ko and ja bring the English word list"
    );

    let ja = found.packs.iter().find(|f| f.manifest.id == "ja").unwrap();
    let dictionary = std::fs::read(ja.dir.join("ipadic_compact.bin")).unwrap();
    assert!(tacit_core::KanjiDictionary::valid(&dictionary));

    let on = tacit_config::languages::enabled(&env);
    assert_eq!(on, Languages::from_ids(&["ko", "ja"]).unwrap());

    cli::run(&args("remove ja"), &env, &source).unwrap();
    assert!(pack::scan(&env.packs)
        .packs
        .iter()
        .all(|f| f.manifest.id != "ja"));
    let _ = std::fs::remove_dir_all(&root);
}
