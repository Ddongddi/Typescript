//! Where the files live. Everything takes an `Env` so tests can point it at a temp folder.

use std::path::PathBuf;

use typescript_core::{pack_dirs, AppPolicies, Settings};

#[derive(Clone, Debug)]
pub struct Env {
    pub settings: PathBuf,
    pub apps: PathBuf,
    /// `<...>/Typescript/packs`: one folder per installed pack version.
    pub packs: PathBuf,
}

impl Env {
    /// The current user's real locations (Windows today; None elsewhere until those platforms
    /// are added).
    pub fn detect() -> Option<Env> {
        Some(Env {
            settings: Settings::store_path()?,
            apps: AppPolicies::store_path()?,
            packs: pack_dirs().into_iter().next()?,
        })
    }

    /// Where a half-installed pack waits: next to `packs`, never inside it (the text service
    /// scans every folder in `packs`).
    pub fn staging(&self) -> PathBuf {
        self.packs
            .parent()
            .map(|p| p.join("packs-incoming"))
            .unwrap_or_else(|| self.packs.join("..").join("packs-incoming"))
    }

    pub fn pack_folder(&self, id: &str, version: &str) -> PathBuf {
        self.packs.join(format!("{id}-{version}"))
    }
}
