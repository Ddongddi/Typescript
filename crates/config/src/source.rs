//! Which release the packs come from: GitHub's latest, another URL, or a folder.

use std::path::Path;

use crate::catalog::Catalog;
use crate::error::{err, Result};
use crate::fetch::{Dir, Fetch, Http};

pub const GITHUB_LATEST: &str =
    "https://github.com/iamallyouneed/Typescript/releases/latest/download/";
const CATALOG_NAME: &str = "packs.json";
const CATALOG_MAX: usize = 1024 * 1024;

pub struct Source {
    pub fetch: Box<dyn Fetch>,
    /// The location of packs.json; pack urls are relative to it.
    pub base: String,
}

impl Source {
    pub fn github() -> Source {
        Source::from_spec(GITHUB_LATEST)
    }

    /// `spec`: a folder holding packs.json and the zips, or a URL (ending in `/`) that does.
    pub fn from_spec(spec: &str) -> Source {
        if spec.contains("://") {
            let dir = if spec.ends_with('/') {
                spec.to_string()
            } else {
                format!("{spec}/")
            };
            Source {
                fetch: Box::new(Http::default()),
                base: format!("{dir}{CATALOG_NAME}"),
            }
        } else {
            let folder = Path::new(spec);
            Source {
                fetch: Box::new(Dir(folder.to_path_buf())),
                base: folder.join(CATALOG_NAME).to_string_lossy().into_owned(),
            }
        }
    }

    /// `TYPESCRIPT_RELEASE` overrides GitHub (offline installs, testing a release by hand).
    pub fn detect() -> Source {
        match std::env::var("TYPESCRIPT_RELEASE") {
            Ok(spec) if !spec.is_empty() => Source::from_spec(&spec),
            _ => Source::github(),
        }
    }

    pub fn catalog(&self) -> Result<Catalog> {
        let bytes = self.fetch.get(&self.base, CATALOG_MAX)?;
        let catalog = Catalog::parse(&bytes)?;
        if !catalog.compatible() {
            return err(format!(
                "the language packs of this release are for typescript {}, this tool is {}: update Typescript first",
                catalog.core,
                crate::catalog::CORE_VERSION
            ));
        }
        Ok(catalog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_gets_packs_json_appended() {
        assert_eq!(
            Source::from_spec("https://x.test/rel").base,
            "https://x.test/rel/packs.json"
        );
        assert_eq!(
            Source::from_spec("https://x.test/rel/").base,
            "https://x.test/rel/packs.json"
        );
    }

    #[test]
    fn a_folder_serves_its_own_catalog() {
        let f = crate::testutil::fixture("source");
        let catalog = crate::testutil::publish(&f).catalog().unwrap();
        assert_eq!(catalog.packs.len(), 3);
    }

    #[test]
    fn a_catalog_for_another_version_asks_for_an_update() {
        let f = crate::testutil::fixture("oldcore");
        let json = r#"{ "version": "9.0.0", "core": ">=9.0.0, <10.0.0", "packs": [] }"#;
        std::fs::write(f.root.join("release").join("packs.json"), json).unwrap();
        let source = Source::from_spec(f.root.join("release").to_str().unwrap());
        let e = source.catalog().unwrap_err();
        assert!(e.0.contains("update"), "{e}");
    }
}
