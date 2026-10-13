//! Where bytes come from: the release on GitHub, or a folder (tests, offline installs).

use std::path::PathBuf;

use crate::error::{err, Error, Result};

pub trait Fetch: Send + Sync {
    /// The bytes at `url`, failing if there are more than `max`.
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>>;
}

pub struct Http;

impl Fetch for Http {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>> {
        let mut response = ureq::get(url)
            .header("User-Agent", "typescript-config")
            .call()
            .map_err(|e| Error(format!("download of {url} failed: {e}")))?;
        response
            .body_mut()
            .with_config()
            .limit(max as u64)
            .read_to_vec()
            .map_err(|e| Error(format!("download of {url} failed: {e}")))
    }
}

/// A folder standing in for a release: `url` is looked up by its last path segment.
pub struct Dir(pub PathBuf);

impl Fetch for Dir {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>> {
        let name = url.rsplit(['/', '\\']).next().unwrap_or(url);
        let bytes = std::fs::read(self.0.join(name)).map_err(|e| Error(format!("{name}: {e}")))?;
        if bytes.len() > max {
            return err(format!("{name} is larger than expected"));
        }
        Ok(bytes)
    }
}

/// `rel` against the location of `base` (a URL or path of a file): an absolute URL stays as is.
pub fn resolve(base: &str, rel: &str) -> String {
    if rel.contains("://") {
        return rel.to_string();
    }
    match base.rfind(['/', '\\']) {
        Some(i) => format!("{}{rel}", &base[..=i]),
        None => rel.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_against_the_base() {
        assert_eq!(
            resolve("https://x.test/rel/packs.json", "en.zip"),
            "https://x.test/rel/en.zip"
        );
        assert_eq!(
            resolve("https://x.test/a/packs.json", "https://y.test/z.zip"),
            "https://y.test/z.zip"
        );
        assert_eq!(resolve("packs.json", "en.zip"), "en.zip");
    }

    #[test]
    fn a_folder_serves_files_by_name_and_respects_the_cap() {
        let dir = std::env::temp_dir().join(format!("ts-cfg-fetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.bin"), [1u8; 10]).unwrap();
        let source = Dir(dir.clone());
        assert_eq!(
            source.get("https://x.test/any/a.bin", 10).unwrap().len(),
            10
        );
        assert!(source.get("a.bin", 9).is_err());
        assert!(source.get("missing.bin", 100).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
