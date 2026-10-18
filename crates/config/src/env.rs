//! Where the files live. Everything takes an `Env` so tests can point it at a temp folder.

use std::path::PathBuf;

use tacit_core::{pack_dirs, AppPolicies, Settings};

#[derive(Clone, Debug)]
pub struct Env {
    pub settings: PathBuf,
    pub apps: PathBuf,
    /// `<...>/tacit/packs`: one folder per installed pack version.
    pub packs: PathBuf,
}

impl Env {
    /// The current user's real locations (None when the system gives no home folder).
    pub fn detect() -> Option<Env> {
        // TACIT_HOME: keep everything in one folder (trying the tool without touching the
        // real settings and packs).
        if let Some(home) = std::env::var_os("TACIT_HOME") {
            let home = PathBuf::from(home);
            return Some(Env {
                settings: home.join("settings.json"),
                apps: home.join("apps.json"),
                packs: home.join("packs"),
            });
        }
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
