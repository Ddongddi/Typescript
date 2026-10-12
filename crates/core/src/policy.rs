//! Per-app behavior (`apps.json`) and user settings (`settings.json`).
//! Port of AppPolicy.swift + Settings.swift. Apps are keyed by lowercase exe name
//! (`windowsterminal.exe`) instead of a bundle identifier; `"*"` is the user's default.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::support_dir;

/// How eagerly Typescript converts in a given app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppPolicy {
    /// When false, every key passes through (plain English typing).
    pub enabled: bool,
    /// Convert away from the typed English letters only when the engine is at least this
    /// confident. Only pays off where a wrong conversion is expensive (terminals, editors).
    pub conversion_threshold: f64,
    /// Esc belongs to the app (vim, editors): it commits the word and goes through, and
    /// Shift+Esc switches the word's language instead.
    pub escape_to_app: bool,
}

impl AppPolicy {
    /// The "strict" choice: convert only when quite sure (short shell commands and
    /// identifiers collide with Hangul easily).
    pub const STRICT_THRESHOLD: f64 = 0.97;

    pub const STANDARD: AppPolicy = AppPolicy { enabled: true, conversion_threshold: 0.5, escape_to_app: false };
}

impl Default for AppPolicy {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// An `apps.json` entry: only the keys it names replace the default policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Override {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversion_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escape_to_app: Option<bool>,
}

impl Override {
    fn applied_to(&self, base: AppPolicy) -> AppPolicy {
        AppPolicy {
            enabled: self.enabled.unwrap_or(base.enabled),
            conversion_threshold: self.conversion_threshold.unwrap_or(base.conversion_threshold),
            escape_to_app: self.escape_to_app.unwrap_or(base.escape_to_app),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AppPolicies {
    /// Keys are lowercase.
    pub overrides: BTreeMap<String, Override>,
}

impl AppPolicies {
    /// The `apps.json` key of the default policy.
    pub const DEFAULT_KEY: &'static str = "*";

    pub fn new(overrides: impl IntoIterator<Item = (String, Override)>) -> Self {
        Self { overrides: overrides.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect() }
    }

    pub fn store_path() -> Option<PathBuf> {
        support_dir().map(|d| d.join("apps.json"))
    }

    /// Loads user overrides; a missing or unreadable file means none.
    pub fn load(path: Option<&Path>) -> Self {
        let parsed = path
            .and_then(|p| fs::read(p).ok())
            .and_then(|d| serde_json::from_slice::<BTreeMap<String, Override>>(&d).ok());
        Self::new(parsed.unwrap_or_default())
    }

    /// What every app uses unless its own entry says otherwise.
    pub fn default_policy(&self) -> AppPolicy {
        self.overrides.get(Self::DEFAULT_KEY).map_or(AppPolicy::STANDARD, |o| o.applied_to(AppPolicy::STANDARD))
    }

    /// The apps the user listed (the default entry excluded).
    pub fn apps(&self) -> Vec<&str> {
        self.overrides.keys().map(String::as_str).filter(|k| *k != Self::DEFAULT_KEY).collect()
    }

    /// `exe`: the host process's file name, any case.
    pub fn policy(&self, exe: Option<&str>) -> AppPolicy {
        let base = self.default_policy();
        let Some(id) = exe.map(str::to_lowercase) else { return base };
        if id == Self::DEFAULT_KEY {
            return base;
        }
        self.overrides.get(&id).map_or(base, |o| o.applied_to(base))
    }
}

/// User settings from `%APPDATA%\Typescript\settings.json`. A missing file or key means the
/// default; unknown keys are ignored (and kept on save).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Return on an underlined word only confirms it; off, the app gets that Return too.
    pub return_confirms_only: bool,
    /// Key that pauses Typescript in the text field: "hangul" (한/영) or "off".
    pub pause_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { return_confirms_only: true, pause_key: "hangul".to_string() }
    }
}

impl Settings {
    pub fn store_path() -> Option<PathBuf> {
        support_dir().map(|d| d.join("settings.json"))
    }

    pub fn load(path: Option<&Path>) -> Settings {
        let value = path
            .and_then(|p| fs::read(p).ok())
            .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok());
        let defaults = Settings::default();
        let Some(value) = value else { return defaults };
        Settings {
            return_confirms_only: value
                .get("returnConfirmsOnly")
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.return_confirms_only),
            pause_key: value.get("pauseKey").and_then(|v| v.as_str()).map(str::to_string).unwrap_or(defaults.pause_key),
        }
    }

    /// Writes these values over the file's, keeping any keys this version doesn't know.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut object = fs::read(path)
            .ok()
            .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok())
            .and_then(|v| if let serde_json::Value::Object(m) = v { Some(m) } else { None })
            .unwrap_or_default();
        object.insert("returnConfirmsOnly".into(), self.return_confirms_only.into());
        object.insert("pauseKey".into(), self.pause_key.clone().into());
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(&serde_json::Value::Object(object)).map_err(std::io::Error::other)?;
        fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("typescript-{name}-{}-{:?}.json", std::process::id(), std::thread::current().id()))
    }

    #[test]
    fn every_app_starts_at_the_default() {
        let policies = AppPolicies::default();
        assert_eq!(policies.policy(Some("windowsterminal.exe")), AppPolicy::STANDARD);
        assert_eq!(policies.policy(None), AppPolicy::STANDARD);
        assert!(policies.apps().is_empty());
    }

    #[test]
    fn user_entries_sit_on_the_default_and_ignore_case() {
        let path = temp("apps");
        fs::write(
            &path,
            r#"{"*": {"escapeToApp": true}, "WindowsTerminal.exe": {"conversionThreshold": 0.97}, "game.exe": {"enabled": false}}"#,
        )
        .unwrap();
        let policies = AppPolicies::load(Some(&path));
        let _ = fs::remove_file(&path);
        let strict = AppPolicy { conversion_threshold: 0.97, escape_to_app: true, ..AppPolicy::STANDARD };
        assert_eq!(policies.policy(Some("WINDOWSTERMINAL.EXE")), strict);
        assert_eq!(policies.policy(Some("game.exe")), AppPolicy { enabled: false, escape_to_app: true, ..AppPolicy::STANDARD });
        assert_eq!(policies.policy(Some("notepad.exe")), AppPolicy { escape_to_app: true, ..AppPolicy::STANDARD });
        assert_eq!(policies.apps(), vec!["game.exe", "windowsterminal.exe"]);
    }

    #[test]
    fn missing_or_broken_files_mean_defaults() {
        assert!(AppPolicies::load(Some(Path::new("Z:/nope/apps.json"))).overrides.is_empty());
        assert_eq!(Settings::load(Some(Path::new("Z:/nope/settings.json"))), Settings::default());
    }

    #[test]
    fn settings_round_trip_keeps_unknown_keys() {
        let path = temp("settings");
        fs::write(&path, r#"{"future": 1, "returnConfirmsOnly": false}"#).unwrap();
        let mut settings = Settings::load(Some(&path));
        assert!(!settings.return_confirms_only);
        settings.pause_key = "off".into();
        settings.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert!(text.contains("\"future\""), "{text}");
        assert!(text.contains("\"off\""), "{text}");
    }
}
