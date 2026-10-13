//! Where bytes come from: the release on GitHub, or a folder (tests, offline installs).

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{err, Error, Result};

pub trait Fetch: Send + Sync {
    /// Bytes come over the network (so hosts are worth checking), not from a folder.
    fn is_remote(&self) -> bool {
        false
    }

    /// The bytes at `url`, failing if there are more than `max`.
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>>;
}

/// HTTPS downloads that give up: a connection that hangs must not hang the program with it.
pub struct Http {
    /// The whole request, connection to last byte.
    pub total: Duration,
}

impl Default for Http {
    fn default() -> Self {
        Http {
            total: Duration::from_secs(120),
        }
    }
}

impl Http {
    /// For the once-a-day look for news, which must never hold a command up.
    pub fn quick() -> Http {
        Http {
            total: Duration::from_secs(8),
        }
    }
}

impl Fetch for Http {
    fn is_remote(&self) -> bool {
        true
    }

    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>> {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(self.total))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut response = agent
            .get(url)
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
