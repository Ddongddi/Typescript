//! Which release the packs come from: GitHub's latest, another URL, or a folder.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::catalog::Catalog;
use crate::error::{err, Result};
use crate::fetch::{Dir, Fetch, Http};

pub const GITHUB_LATEST: &str = "https://github.com/iamallyouneed/tacit/releases/latest/download/";
const CATALOG_NAME: &str = "packs.json";
const CATALOG_MAX: usize = 1024 * 1024;

/// How long a catalog that came over the network is reused.
const CATALOG_KEPT: Duration = Duration::from_secs(300);

pub struct Source {
    pub fetch: Box<dyn Fetch>,
    /// The location of packs.json; pack urls are relative to it.
    pub base: String,
    /// The last catalog from the network and when it came.
    cached: Mutex<Option<(Instant, Catalog)>>,
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
                cached: Mutex::new(None),
            }
        } else {
            let folder = Path::new(spec);
            Source {
                fetch: Box::new(Dir(folder.to_path_buf())),
                base: folder.join(CATALOG_NAME).to_string_lossy().into_owned(),
                cached: Mutex::new(None),
            }
        }
    }

    /// `TACIT_RELEASE` overrides GitHub (offline installs, testing a release by hand).
    pub fn detect() -> Source {
        match std::env::var("TACIT_RELEASE") {
            Ok(spec) if !spec.is_empty() => Source::from_spec(&spec),
            _ => Source::github(),
        }
    }

    /// The release's catalog. One that came over the network is reused for a few minutes (the
    /// dashboard asks on almost every request, and a slow connection would make each one slow).
    pub fn catalog(&self) -> Result<Catalog> {
        let remote = self.fetch.is_remote();
        if remote {
            let kept = self.cached.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((at, catalog)) = kept.as_ref() {
                if at.elapsed() < CATALOG_KEPT {
                    return Ok(catalog.clone());
                }
            }
        }
        let catalog = self.load()?;
        if remote {
            *self.cached.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((Instant::now(), catalog.clone()));
        }
        Ok(catalog)
    }

    /// Drops the kept catalog: the next look asks the network (after an update, a new release).
    pub fn forget(&self) {
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn load(&self) -> Result<Catalog> {
        let bytes = self.fetch.get(&self.base, CATALOG_MAX)?;
        let catalog = Catalog::parse(&bytes)?;
        if !catalog.compatible() {
            return err(format!(
                "the language packs of this release are for Tacit {}, this tool is {}: update Tacit first",
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

    /// A network that counts how often it is asked, answering with a folder's files.
    struct Counting(
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
        crate::fetch::Dir,
    );

    impl Fetch for Counting {
        fn is_remote(&self) -> bool {
            true
        }

        fn get(&self, url: &str, max: usize) -> Result<Vec<u8>> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.1.get(url, max)
        }
    }

    #[test]
    fn a_catalog_from_the_network_is_kept_for_a_while_and_can_be_forgotten() {
        let f = crate::testutil::fixture("source-cache");
        crate::testutil::publish(&f);
        let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let source = Source {
            fetch: Box::new(Counting(
                asked.clone(),
                crate::fetch::Dir(f.root.join("release")),
            )),
            base: "https://x.test/rel/packs.json".into(),
            cached: Mutex::new(None),
        };
        let times = || asked.load(std::sync::atomic::Ordering::SeqCst);
        source.catalog().unwrap();
        source.catalog().unwrap();
        assert_eq!(times(), 1, "the second look reuses the first");
        source.forget();
        source.catalog().unwrap();
        assert_eq!(times(), 2, "after forgetting it asks again");
    }

    #[test]
    fn a_folder_is_read_every_time() {
        let f = crate::testutil::fixture("source-folder");
        let source = crate::testutil::publish(&f);
        assert_eq!(source.catalog().unwrap().packs.len(), 3);
        let json = r#"{ "version": "9.0.0", "core": ">=0.0.0", "packs": [] }"#;
        std::fs::write(f.root.join("release").join("packs.json"), json).unwrap();
        assert!(source.catalog().unwrap().packs.is_empty(), "no stale copy");
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
