//! Writes the language packs into OUT_DIR/packs/<id>/ (the same tables the release packs carry,
//! see crates/packgen). The tests, the CLI and the embedded builds read these; the Windows
//! text service reads the installed packs instead.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data");
    let data = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("data");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("packs");
    for pack in typescript_packgen::PACKS {
        typescript_packgen::write_pack(pack.id, &data, &out.join(pack.id))
            .unwrap_or_else(|e| panic!("pack {}: {e}", pack.id));
    }
}
