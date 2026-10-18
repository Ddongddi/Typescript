//! Fixtures for the tests: a fake release folder with small packs, and an `Env` in a temp dir.

use std::io::Write;
use std::path::PathBuf;

use crate::catalog::{Catalog, CatalogPack};
use crate::env::Env;
use crate::fetch::Dir;
use crate::hash::sha256_hex;

pub struct Fixture {
    pub root: PathBuf,
    pub env: Env,
    pub source: Dir,
    pub catalog: Catalog,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut out);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    out.into_inner()
}

/// A pack zip: manifest.json (with the tables' hashes) and the tables.
pub fn pack_zip(id: &str, version: &str, requires: &[&str], tables: &[(&str, &[u8])]) -> Vec<u8> {
    let mut hashes = serde_json::Map::new();
    for (name, bytes) in tables {
        hashes.insert(name.to_string(), sha256_hex(bytes).into());
    }
    let manifest = serde_json::json!({
        "id": id, "name": id, "version": version, "core": ">=0.4.0, <99.0.0",
        "requires": requires, "files": hashes,
    });
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let mut files: Vec<(&str, &[u8])> = vec![("manifest.json", manifest.as_slice())];
    files.extend_from_slice(tables);
    zip_of(&files)
}

fn catalog_pack(id: &str, version: &str, requires: &[&str], zip: &[u8]) -> CatalogPack {
    CatalogPack {
        id: id.into(),
        name: id.into(),
        version: version.into(),
        requires: requires.iter().map(|r| r.to_string()).collect(),
        size: zip.len() as u64,
        sha256: sha256_hex(zip),
        url: format!("tacit-pack-{id}-{version}.zip"),
    }
}

/// A release with `en` and `ko` (ko needs en) and a `ja` that needs en.
pub fn fixture(name: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!("ts-cfg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let release = root.join("release");
    let home = root.join("home").join("tacit");
    std::fs::create_dir_all(&release).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let mut packs = Vec::new();
    for (id, requires) in [("en", vec![]), ("ko", vec!["en"]), ("ja", vec!["en"])] {
        let table = format!("table of {id}");
        let zip = pack_zip(id, "0.4.0", &requires, &[("table.txt", table.as_bytes())]);
        let pack = catalog_pack(id, "0.4.0", &requires, &zip);
        std::fs::write(release.join(&pack.url), &zip).unwrap();
        packs.push(pack);
    }
    let catalog = Catalog {
        version: "0.4.0".into(),
        core: tacit_packgen::core_range(),
        packs,
    };
    Fixture {
        env: Env {
            settings: home.join("settings.json"),
            apps: home.join("apps.json"),
            packs: root.join("local").join("tacit").join("packs"),
        },
        source: Dir(release),
        catalog,
        root,
    }
}

/// Writes the fixture's catalog as the release's packs.json and returns a source for it.
pub fn publish(f: &Fixture) -> crate::source::Source {
    let json = serde_json::json!({
        "version": f.catalog.version, "core": f.catalog.core,
        "packs": f.catalog.packs.iter().map(|p| serde_json::json!({
            "id": p.id, "name": p.name, "version": p.version, "requires": p.requires,
            "size": p.size, "sha256": p.sha256, "url": p.url })).collect::<Vec<_>>(),
    });
    std::fs::write(f.root.join("release").join("packs.json"), json.to_string()).unwrap();
    crate::source::Source::from_spec(f.root.join("release").to_str().unwrap())
}
