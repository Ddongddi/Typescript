//! Per-app behavior (`apps.json`) and user settings (`settings.json`).
//! Port of AppPolicy.swift + Settings.swift. Apps are keyed by lowercase exe name
//! (`windowsterminal.exe`) instead of a bundle identifier; `"*"` is the user's default.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::decide::engine::Language;
use crate::lang::set::Languages;
use crate::support_dir;

use super::keys::{self, Chord, ChordKey, HangulAction, KeyAction, Mods};

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
    /// Typescript steps aside in this app: the text service hands the process over to the
    /// system's Korean IME, so the Han/Eng key works as it does everywhere else. For apps that
    /// don't get along with composition-based input (KakaoTalk).
    pub use_system_ime: bool,
    /// Full-width digits and spaces after Japanese (when turned on in the settings) apply here;
    /// false keeps them half-width in this app (a terminal, a code editor).
    pub full_width: bool,
    /// Return on an underlined word only confirms it (true) or goes to the app too (false) in
    /// this app; None follows the `returnConfirmsOnly` setting. Chat apps send on one Return.
    pub return_confirms_only: Option<bool>,
}

impl AppPolicy {
    /// The "strict" choice: convert only when quite sure (short shell commands and
    /// identifiers collide with Hangul easily).
    pub const STRICT_THRESHOLD: f64 = 0.97;

    pub const STANDARD: AppPolicy = AppPolicy {
        enabled: true,
        conversion_threshold: 0.5,
        escape_to_app: false,
        use_system_ime: false,
        full_width: true,
        return_confirms_only: None,
    };
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_system_ime: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_width: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_confirms_only: Option<bool>,
}

impl Override {
    fn applied_to(&self, base: AppPolicy) -> AppPolicy {
        AppPolicy {
            enabled: self.enabled.unwrap_or(base.enabled),
            conversion_threshold: self
                .conversion_threshold
                .unwrap_or(base.conversion_threshold),
            escape_to_app: self.escape_to_app.unwrap_or(base.escape_to_app),
            use_system_ime: self.use_system_ime.unwrap_or(base.use_system_ime),
            full_width: self.full_width.unwrap_or(base.full_width),
            return_confirms_only: self.return_confirms_only.or(base.return_confirms_only),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AppPolicies {
    /// Keys are lowercase.
    pub overrides: BTreeMap<String, Override>,
}

/// Terminals: short commands (`ls`, `git`) collide with Hangul, so convert only when sure.
const TERMINALS: &[&str] = &[
    "windowsterminal.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "conhost.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "mintty.exe",
    "terminal",
    "iterm2",
];
/// Editors where Esc belongs to the app (vim, normal mode in VS Code).
const EDITORS: &[&str] = &[
    "code.exe",
    "code - insiders.exe",
    "cursor.exe",
    "windsurf.exe",
    "devenv.exe",
    "idea64.exe",
    "pycharm64.exe",
    "webstorm64.exe",
    "clion64.exe",
    "sublime_text.exe",
    "gvim.exe",
    "nvim-qt.exe",
    "vscode",
    "cursor",
    "sublime_text",
];

/// Chat apps: one Return sends the message, so it must not be spent on confirming the word.
const MESSENGERS: &[&str] = &[
    "slack.exe",
    "discord.exe",
    "ms-teams.exe",
    "teams.exe",
    "telegram.exe",
    "line.exe",
    "kakaotalk.exe",
    "slack",
    "discord",
    "telegram",
    "line",
];

/// What we know about an app without the user saying anything. The user's own entry for the
/// app is applied on top, so any of this can be turned off in `apps.json`.
fn builtin(id: &str) -> Option<Override> {
    if TERMINALS.contains(&id) {
        return Some(Override {
            conversion_threshold: Some(AppPolicy::STRICT_THRESHOLD),
            full_width: Some(false),
            ..Override::default()
        });
    }
    if MESSENGERS.contains(&id) {
        return Some(Override {
            return_confirms_only: Some(false),
            ..Override::default()
        });
    }
    EDITORS.contains(&id).then(|| Override {
        escape_to_app: Some(true),
        full_width: Some(false),
        ..Override::default()
    })
}

/// The file's bytes without a leading UTF-8 byte order mark: Windows PowerShell and Notepad
/// write one, and serde_json refuses it.
fn read_json_file(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    let bom = [0xEF, 0xBB, 0xBF];
    Some(
        data.strip_prefix(&bom[..])
            .map_or_else(|| data.clone(), <[u8]>::to_vec),
    )
}

impl AppPolicies {
    /// The `apps.json` key of the default policy.
    pub const DEFAULT_KEY: &'static str = "*";

    pub fn new(overrides: impl IntoIterator<Item = (String, Override)>) -> Self {
        Self {
            overrides: overrides
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v))
                .collect(),
        }
    }

    pub fn store_path() -> Option<PathBuf> {
        support_dir().map(|d| d.join("apps.json"))
    }

    /// Loads user overrides; a missing or unreadable file means none.
    pub fn load(path: Option<&Path>) -> Self {
        let parsed = path
            .and_then(read_json_file)
            .and_then(|d| serde_json::from_slice::<BTreeMap<String, Override>>(&d).ok());
        Self::new(parsed.unwrap_or_default())
    }

    /// What every app uses unless its own entry says otherwise.
    pub fn default_policy(&self) -> AppPolicy {
        self.overrides
            .get(Self::DEFAULT_KEY)
            .map_or(AppPolicy::STANDARD, |o| o.applied_to(AppPolicy::STANDARD))
    }

    /// The apps the user listed (the default entry excluded).
    pub fn apps(&self) -> Vec<&str> {
        self.overrides
            .keys()
            .map(String::as_str)
            .filter(|k| *k != Self::DEFAULT_KEY)
            .collect()
    }

    /// `exe`: the host process's file name, any case.
    pub fn policy(&self, exe: Option<&str>) -> AppPolicy {
        let base = self.default_policy();
        let Some(id) = exe.map(str::to_lowercase) else {
            return base;
        };
        if id == Self::DEFAULT_KEY {
            return base;
        }
        let base = builtin(&id).map_or(base, |o| o.applied_to(base));
        self.overrides.get(&id).map_or(base, |o| o.applied_to(base))
    }
}

/// The list under `name`: None when it is missing, not a list, or has entries but none usable
/// (the caller then uses its default; a hand-edit must not switch a key off by accident).
fn key_list(
    value: &serde_json::Value,
    name: &str,
    hangul: &Chord,
    taken: &[Chord],
) -> Option<Vec<Chord>> {
    let list = value.get(name)?.as_array()?;
    let texts: Vec<&str> = list.iter().filter_map(|v| v.as_str()).collect();
    let kept = keys::sanitize(&texts, hangul, taken);
    (list.is_empty() || !kept.is_empty()).then_some(kept)
}

/// User settings from `%APPDATA%\Typescript\settings.json`. A missing file or key means the
/// default; unknown keys are ignored (and kept on save).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Return on an underlined word only confirms it; off, the app gets that Return too.
    pub return_confirms_only: bool,
    /// The Han/Eng key: any key; `HangulKey` in the lists below stands for it.
    pub hangul_key: Chord,
    /// What the Han/Eng key does on its own: switch the word's language, nothing, or send a
    /// shortcut.
    pub hangul_action: HangulAction,
    /// Keys that switch the word being typed to its next language.
    pub flip_keys: Vec<Chord>,
    /// Keys that pause Typescript in the text field. Empty: no pause key.
    pub pause_keys: Vec<Chord>,
    /// Tab completes the word being typed, and the words typed are counted for that.
    pub autocomplete: bool,
    /// The languages turned on, in Esc order (`"languages": ["en", "ko"]`). Files from before
    /// language packs have only `"japanese"`: that gives English + Korean (+ Japanese).
    pub languages: Languages,
    /// With Japanese on: show (and commit) the top kanji candidate while typing, like macOS
    /// Live Conversion. Space opens the other candidates, Esc shows the kana.
    pub live_conversion: bool,
    /// With Japanese on: digits after Japanese text are part of it, full-width (１２３);
    /// Space offers 123.
    pub full_width_digits: bool,
    /// With Japanese on: a space after Japanese text is full-width (⇧Space types the other width).
    pub full_width_space: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            return_confirms_only: true,
            hangul_key: keys::default_hangul(),
            hangul_action: HangulAction::Flip,
            flip_keys: keys::default_flip(),
            pause_keys: keys::default_pause(),
            autocomplete: true,
            languages: Languages::legacy(false),
            live_conversion: false,
            full_width_digits: false,
            full_width_space: false,
        }
    }
}

impl Settings {
    pub fn store_path() -> Option<PathBuf> {
        support_dir().map(|d| d.join("settings.json"))
    }

    /// What pressing `pressed` with exactly `mods` held does, if it is an assigned key.
    pub fn key_action(&self, mods: Mods, pressed: ChordKey) -> Option<KeyAction> {
        let hit = |list: &[Chord]| {
            list.iter().any(|c| {
                c.resolve(&self.hangul_key)
                    .is_ok_and(|real| real.matches(mods, pressed))
            })
        };
        if hit(&self.pause_keys) {
            Some(KeyAction::Pause)
        } else if self.hangul_key.matches(mods, pressed) && self.hangul_action != HangulAction::Flip
        {
            // the Han/Eng key itself, when it is not the word-language key
            match self.hangul_action {
                HangulAction::Send(_) => Some(KeyAction::Send),
                _ => None,
            }
        } else if hit(&self.flip_keys) {
            Some(KeyAction::Flip)
        } else {
            None
        }
    }

    pub fn load(path: Option<&Path>) -> Settings {
        let value = path
            .and_then(read_json_file)
            .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok());
        let defaults = Settings::default();
        let Some(value) = value else { return defaults };
        let hangul_key = value
            .get("hangulKey")
            .and_then(|v| v.as_str())
            .and_then(|text| keys::parse_hangul(text).ok())
            .unwrap_or_else(keys::default_hangul);
        let hangul_action = value
            .get("hangulAction")
            .and_then(|v| v.as_str())
            .and_then(|text| text.parse().ok())
            .unwrap_or(HangulAction::Flip);
        let flip_keys =
            key_list(&value, "flipKeys", &hangul_key, &[]).unwrap_or_else(keys::default_flip);
        let legacy_off = value.get("pauseKey").and_then(|v| v.as_str()) == Some("off");
        let pause_keys =
            key_list(&value, "pauseKeys", &hangul_key, &flip_keys).unwrap_or_else(|| {
                if legacy_off {
                    Vec::new()
                } else {
                    keys::default_pause()
                }
            });
        Settings {
            return_confirms_only: value
                .get("returnConfirmsOnly")
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.return_confirms_only),
            hangul_key,
            hangul_action,
            flip_keys,
            pause_keys,
            autocomplete: value
                .get("autocomplete")
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.autocomplete),
            languages: value
                .get("languages")
                .and_then(|v| v.as_array())
                .and_then(|list| {
                    let ids: Vec<&str> = list.iter().filter_map(|v| v.as_str()).collect();
                    Languages::from_ids(&ids)
                })
                .unwrap_or_else(|| {
                    Languages::legacy(
                        value
                            .get("japanese")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    )
                }),
            live_conversion: value
                .get("liveConversion")
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.live_conversion),
            full_width_digits: value
                .get("fullWidthDigits")
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.full_width_digits),
            full_width_space: value
                .get("fullWidthSpace")
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.full_width_space),
        }
    }

    /// Writes these values over the file's, keeping any keys this version doesn't know.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut object = read_json_file(path)
            .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok())
            .and_then(|v| {
                if let serde_json::Value::Object(m) = v {
                    Some(m)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        object.insert(
            "returnConfirmsOnly".into(),
            self.return_confirms_only.into(),
        );
        let names =
            |list: &[Chord]| -> Vec<String> { list.iter().map(|c| c.to_string()).collect() };
        object.remove("pauseKey");
        object.insert("hangulKey".into(), self.hangul_key.to_string().into());
        object.insert("hangulAction".into(), self.hangul_action.to_string().into());
        object.insert("flipKeys".into(), names(&self.flip_keys).into());
        object.insert("pauseKeys".into(), names(&self.pause_keys).into());
        object.insert("autocomplete".into(), self.autocomplete.into());
        object.insert("languages".into(), self.languages.ids().into());
        // Read by the versions before language packs.
        object.insert(
            "japanese".into(),
            self.languages.contains(Language::Japanese).into(),
        );
        object.insert("liveConversion".into(), self.live_conversion.into());
        object.insert("fullWidthDigits".into(), self.full_width_digits.into());
        object.insert("fullWidthSpace".into(), self.full_width_space.into());
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(&serde_json::Value::Object(object))
            .map_err(std::io::Error::other)?;
        super::write_atomic(path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "typescript-{name}-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[test]
    fn every_app_starts_at_the_default() {
        let policies = AppPolicies::default();
        assert_eq!(policies.policy(Some("notepad.exe")), AppPolicy::STANDARD);
        assert_eq!(policies.policy(None), AppPolicy::STANDARD);
        assert!(policies.apps().is_empty());
    }

    #[test]
    fn terminals_and_editors_have_sensible_rules_the_user_can_override() {
        let none = AppPolicies::default();
        let terminal = none.policy(Some("WindowsTerminal.exe"));
        assert_eq!(terminal.conversion_threshold, AppPolicy::STRICT_THRESHOLD);
        assert!(!terminal.escape_to_app && !terminal.full_width);
        let chat = none.policy(Some("Slack.exe"));
        assert_eq!(chat.return_confirms_only, Some(false));
        assert_eq!(none.policy(Some("notepad.exe")).return_confirms_only, None);
        let editor = none.policy(Some("Code.exe"));
        assert!(editor.escape_to_app && editor.enabled);
        assert_eq!(editor.conversion_threshold, 0.5);
        let mine = AppPolicies::new([(
            "code.exe".to_string(),
            Override {
                escape_to_app: Some(false),
                ..Override::default()
            },
        )]);
        assert!(!mine.policy(Some("code.exe")).escape_to_app);
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
        let strict = AppPolicy {
            conversion_threshold: 0.97,
            escape_to_app: true,
            full_width: false, // a terminal's own rule
            ..AppPolicy::STANDARD
        };
        assert_eq!(policies.policy(Some("WINDOWSTERMINAL.EXE")), strict);
        assert_eq!(
            policies.policy(Some("game.exe")),
            AppPolicy {
                enabled: false,
                escape_to_app: true,
                ..AppPolicy::STANDARD
            }
        );
        assert_eq!(
            policies.policy(Some("notepad.exe")),
            AppPolicy {
                escape_to_app: true,
                ..AppPolicy::STANDARD
            }
        );
        assert_eq!(policies.apps(), vec!["game.exe", "windowsterminal.exe"]);
    }

    #[test]
    fn use_system_ime_is_per_app() {
        let policies = AppPolicies::new([(
            "KakaoTalk.exe".to_string(),
            Override {
                use_system_ime: Some(true),
                ..Default::default()
            },
        )]);
        assert!(policies.policy(Some("kakaotalk.exe")).use_system_ime);
        assert!(!policies.policy(Some("notepad.exe")).use_system_ime);
        let parsed: Override = serde_json::from_str(r#"{"useSystemIme": true}"#).unwrap();
        assert_eq!(parsed.use_system_ime, Some(true));
    }

    #[test]
    fn missing_or_broken_files_mean_defaults() {
        assert!(AppPolicies::load(Some(Path::new("Z:/nope/apps.json")))
            .overrides
            .is_empty());
        assert_eq!(
            Settings::load(Some(Path::new("Z:/nope/settings.json"))),
            Settings::default()
        );
    }

    #[test]
    fn settings_round_trip_keeps_unknown_keys() {
        let path = temp("settings");
        fs::write(
            &path,
            r#"{"future": 1, "returnConfirmsOnly": false, "pauseKey": "hangul"}"#,
        )
        .unwrap();
        let mut settings = Settings::load(Some(&path));
        assert!(!settings.return_confirms_only);
        settings.pause_keys = Vec::new();
        settings.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert!(text.contains("\"future\""), "{text}");
        assert!(text.contains("\"pauseKeys\": []"), "{text}");
        assert!(text.contains("\"flipKeys\""), "{text}");
        assert!(
            !text.contains("\"pauseKey\""),
            "the old key is gone: {text}"
        );
    }

    fn shown(list: &[keys::Chord]) -> Vec<String> {
        list.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn keys_default_when_the_file_says_nothing() {
        let s = load_json("keys-default", "{}");
        assert_eq!(shown(&s.flip_keys), ["Esc", "HangulKey"]);
        assert_eq!(shown(&s.pause_keys), ["Alt+HangulKey"]);
        assert_eq!(s.hangul_key.to_string(), "Hangul");
    }

    #[test]
    fn the_han_eng_key_can_be_another_key_and_the_lists_follow_it() {
        let s = load_json("keys-hangul", r#"{ "hangulKey": "RAlt" }"#);
        assert_eq!(s.hangul_key.to_string(), "RAlt");
        use keys::{ChordKey, KeyAction, Mods};
        assert_eq!(
            s.key_action(Mods::NONE, ChordKey::RAlt),
            Some(KeyAction::Flip)
        );
        assert_eq!(s.key_action(Mods::NONE, ChordKey::Hangul), None);
        // Alt + a modifier key can't be built: the pause key is then simply missing
        let alt = Mods {
            alt: true,
            ..Mods::NONE
        };
        assert_eq!(s.key_action(alt, ChordKey::RAlt), None);
        let s = load_json("keys-hangul-f13", r#"{ "hangulKey": "F13" }"#);
        let alt = Mods {
            alt: true,
            ..Mods::NONE
        };
        assert_eq!(s.key_action(alt, ChordKey::F(13)), Some(KeyAction::Pause));
    }

    #[test]
    fn the_han_eng_key_can_send_a_shortcut_or_do_nothing() {
        use keys::{ChordKey, KeyAction, Mods};
        let s = load_json("keys-send", r#"{ "hangulAction": "ctrl+c" }"#);
        assert_eq!(s.hangul_action.to_string(), "Ctrl+C");
        assert_eq!(
            s.key_action(Mods::NONE, ChordKey::Hangul),
            Some(KeyAction::Send)
        );
        // Esc is still a language key; Alt+Han/Eng is still the pause key
        assert_eq!(
            s.key_action(Mods::NONE, ChordKey::Esc),
            Some(KeyAction::Flip)
        );
        let alt = Mods {
            alt: true,
            ..Mods::NONE
        };
        assert_eq!(s.key_action(alt, ChordKey::Hangul), Some(KeyAction::Pause));
        let s = load_json("keys-none", r#"{ "hangulAction": "none" }"#);
        assert_eq!(s.key_action(Mods::NONE, ChordKey::Hangul), None);
        assert_eq!(
            s.key_action(Mods::NONE, ChordKey::Esc),
            Some(KeyAction::Flip)
        );
        // junk means the default: the key switches the word's language
        let s = load_json("keys-junk-action", r#"{ "hangulAction": "ctrl+nothing" }"#);
        assert_eq!(
            s.key_action(Mods::NONE, ChordKey::Hangul),
            Some(KeyAction::Flip)
        );
    }

    #[test]
    fn the_old_pause_key_off_stays_off() {
        let s = load_json("keys-legacy-off", r#"{ "pauseKey": "off" }"#);
        assert!(s.pause_keys.is_empty());
        let s = load_json("keys-legacy-on", r#"{ "pauseKey": "hangul" }"#);
        assert_eq!(shown(&s.pause_keys), ["Alt+HangulKey"]);
    }

    #[test]
    fn an_empty_list_means_off_but_nothing_usable_means_default() {
        let s = load_json("keys-empty", r#"{ "flipKeys": [], "pauseKeys": [] }"#);
        assert!(s.flip_keys.is_empty() && s.pause_keys.is_empty());
        let s = load_json(
            "keys-junk",
            r#"{ "flipKeys": ["Win+L", 3], "pauseKeys": "x" }"#,
        );
        assert_eq!(shown(&s.flip_keys), ["Esc", "HangulKey"]);
        assert_eq!(shown(&s.pause_keys), ["Alt+HangulKey"]);
    }

    #[test]
    fn a_hand_written_clash_gives_the_key_to_the_language_switch() {
        let s = load_json(
            "keys-clash",
            r#"{ "flipKeys": ["Esc", "F13"], "pauseKeys": ["F13", "Ctrl"] }"#,
        );
        assert_eq!(shown(&s.pause_keys), ["Ctrl"]);
        assert_eq!(
            s.key_action(keys::Mods::NONE, keys::ChordKey::F(13)),
            Some(keys::KeyAction::Flip)
        );
    }

    fn load_json(name: &str, json: &str) -> Settings {
        let path = temp(name);
        std::fs::write(&path, json).unwrap();
        let s = Settings::load(Some(&path));
        let _ = std::fs::remove_file(&path);
        s
    }

    #[test]
    fn languages_come_from_the_list_in_order() {
        let s = load_json("langs-list", r#"{ "languages": ["ko", "ja"] }"#);
        assert_eq!(s.languages.ids(), vec!["ko", "ja"]);
    }

    #[test]
    fn languages_fall_back_to_the_japanese_key() {
        let s = load_json("langs-legacy-ja", r#"{ "japanese": true }"#);
        assert_eq!(s.languages.ids(), vec!["en", "ko", "ja"]);
        let s = load_json("langs-legacy", r#"{ "japanese": false }"#);
        assert_eq!(s.languages.ids(), vec!["en", "ko"]);
    }

    #[test]
    fn broken_languages_fall_back_too() {
        for json in [
            r#"{ "japanese": false, "languages": [] }"#,
            r#"{ "japanese": false, "languages": ["xx"] }"#,
            r#"{ "japanese": false, "languages": ["mixed"] }"#,
            r#"{ "japanese": false, "languages": "ko" }"#,
            r#"{ "japanese": false, "languages": [1, 2] }"#,
        ] {
            let s = load_json("langs-broken", json);
            assert_eq!(s.languages.ids(), vec!["en", "ko"], "{json}");
        }
    }

    #[test]
    fn save_writes_languages_and_the_old_japanese_key() {
        let path = temp("langs-save");
        let _ = std::fs::remove_file(&path);
        let s = Settings {
            languages: Languages::from_ids(&["ja", "en"]).unwrap(),
            ..Settings::default()
        };
        s.save(&path).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["languages"], serde_json::json!(["ja", "en"]));
        assert_eq!(v["japanese"], serde_json::json!(true));
        assert_eq!(
            Settings::load(Some(&path)).languages.ids(),
            vec!["ja", "en"]
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_file() {
        // Windows PowerShell and Notepad write settings.json and apps.json with a BOM.
        let s = load_json("langs-bom", "\u{feff}{ \"languages\": [\"ko\"] }");
        assert_eq!(s.languages.ids(), vec!["ko"]);
        let path = temp("apps-bom");
        std::fs::write(&path, "\u{feff}{ \"game.exe\": { \"enabled\": false } }").unwrap();
        let policies = AppPolicies::load(Some(&path));
        assert!(!policies.policy(Some("game.exe")).enabled);
        let _ = std::fs::remove_file(&path);
    }
}
