//! `packs.json`, the release's list of language packs.

use serde::Deserialize;
use typescript_core::pack::version_matches;

use crate::error::{err, Result};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CatalogPack {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub requires: Vec<String>,
    pub size: u64,
    pub sha256: String,
    /// Relative to the packs.json location, or absolute.
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Catalog {
    pub version: String,
    pub core: String,
    pub packs: Vec<CatalogPack>,
}

/// The version of the core this tool was built with (the pack's `core` range is checked against it).
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Catalog {
    pub fn parse(bytes: &[u8]) -> Result<Catalog> {
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
        let catalog: Catalog = serde_json::from_slice(bytes)?;
        for p in &catalog.packs {
            if !valid_id(&p.id) || !valid_version(&p.version) {
                return err("packs.json: a pack has an invalid id or version");
            }
            if p.size == 0 || p.size > MAX_PACK_BYTES {
                return err(format!("packs.json: {} has an unreasonable size", p.id));
            }
        }
        Ok(catalog)
    }

    /// This tool works with the packs the catalog lists.
    pub fn compatible(&self) -> bool {
        version_matches(&self.core, CORE_VERSION)
    }

    pub fn pack(&self, id: &str) -> Option<&CatalogPack> {
        self.packs.iter().find(|p| p.id == id)
    }

    /// `id` and the packs it needs, the needed ones first, nothing twice.
    pub fn with_requirements(&self, id: &str) -> Result<Vec<&CatalogPack>> {
        let mut out: Vec<&CatalogPack> = Vec::new();
        self.collect(id, &mut out, 0)?;
        Ok(out)
    }

    fn collect<'a>(&'a self, id: &str, out: &mut Vec<&'a CatalogPack>, depth: usize) -> Result<()> {
        if depth > 8 {
            return err("packs.json: requirements nest too deep");
        }
        let Some(pack) = self.pack(id) else {
            return err(format!("no language pack '{id}' in this release"));
        };
        if out.iter().any(|p| p.id == pack.id) {
            return Ok(());
        }
        for required in &pack.requires {
            self.collect(required, out, depth + 1)?;
        }
        out.push(pack);
        Ok(())
    }
}

/// The most a single pack download may be (the largest real one is a few MB).
pub const MAX_PACK_BYTES: u64 = 256 * 1024 * 1024;

/// A version becomes part of a folder name too: digits, letters, dots, dashes, no `..`.
pub fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 32
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        && !version.contains("..")
        && !version.starts_with('.')
        && !version.ends_with('.')
}

/// Pack ids become folder names: lowercase letters and digits only.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 16
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(extra: &str) -> String {
        let core = typescript_packgen::core_range();
        format!(
            r#"{{ "version": "0.4.0", "core": "{core}", "packs": [
              {{ "id": "en", "name": "English", "version": "0.4.0", "size": 10, "sha256": "aa", "url": "en.zip" }},
              {{ "id": "ko", "name": "한국어", "version": "0.4.0", "requires": ["en"], "size": 10, "sha256": "bb", "url": "ko.zip" }}
              {extra} ] }}"#
        )
    }

    #[test]
    fn parses_and_finds_packs() {
        let c = Catalog::parse(json("").as_bytes()).unwrap();
        assert_eq!(c.packs.len(), 2);
        assert!(c.compatible());
        assert_eq!(c.pack("ko").unwrap().requires, vec!["en"]);
        assert!(c.pack("xx").is_none());
    }

    #[test]
    fn a_byte_order_mark_is_fine() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(json("").as_bytes());
        assert!(Catalog::parse(&bytes).is_ok());
    }

    #[test]
    fn requirements_come_first_and_once() {
        let c = Catalog::parse(json("").as_bytes()).unwrap();
        let ids: Vec<&str> = c
            .with_requirements("ko")
            .unwrap()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(ids, vec!["en", "ko"]);
        assert_eq!(c.with_requirements("en").unwrap().len(), 1);
        assert!(c.with_requirements("xx").is_err());
    }

    #[test]
    fn a_catalog_for_another_core_is_incompatible() {
        let c = Catalog::parse(
            json("")
                .replace(&typescript_packgen::core_range(), ">=99.0.0")
                .as_bytes(),
        )
        .unwrap();
        assert!(!c.compatible());
    }

    #[test]
    fn ids_that_could_escape_a_folder_are_refused() {
        let bad = json(
            r#", { "id": "../x", "name": "x", "version": "1", "size": 1, "sha256": "c", "url": "x.zip" }"#,
        );
        assert!(Catalog::parse(bad.as_bytes()).is_err());
        assert!(!valid_id("a/b") && !valid_id("") && !valid_id("En"));
    }

    #[test]
    fn requirement_cycles_stop() {
        let looped = r#"{ "version": "1", "core": ">=0.4.0", "packs": [
          { "id": "a", "name": "a", "version": "1", "requires": ["b"], "size": 1, "sha256": "x", "url": "a.zip" },
          { "id": "b", "name": "b", "version": "1", "requires": ["a"], "size": 1, "sha256": "x", "url": "b.zip" } ] }"#;
        let c = Catalog::parse(looped.as_bytes()).unwrap();
        assert!(c.with_requirements("a").is_err());
    }

    #[test]
    fn a_version_that_could_leave_the_packs_folder_is_refused() {
        for version in ["1/../../x", "..", "1\u{5c}2", "a b", "", "1.0.0/", "C:x"] {
            let bad = json("").replace(
                r#""version": "0.4.0", "size": 10, "sha256": "aa""#,
                &format!(
                    r#""version": {}, "size": 10, "sha256": "aa""#,
                    serde_json::to_string(version).unwrap()
                ),
            );
            assert!(Catalog::parse(bad.as_bytes()).is_err(), "{version}");
        }
        assert!(valid_version("0.4.0") && valid_version("1.2.3-rc.1"));
    }

    #[test]
    fn a_pack_cannot_claim_a_huge_download() {
        let big = json("").replace(
            r#""size": 10, "sha256": "aa""#,
            r#""size": 99999999999, "sha256": "aa""#,
        );
        assert!(Catalog::parse(big.as_bytes()).is_err());
    }
}
