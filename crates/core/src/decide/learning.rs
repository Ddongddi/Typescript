//! Words the user corrected with Esc (LearningEngine in DecisionEngine.swift): a correction
//! made twice wins over the heuristics. Stored in `%APPDATA%\Typescript\lexicon.json`.
//!
//! Several processes share the file (a TIP lives inside every app), so changes re-read the
//! file first and replace it atomically. ponytail: no cross-process lock; with two
//! simultaneous learns the last write wins. Add a named mutex if that ever matters.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::decide::engine::{Decision, DecisionContext, DecisionEngine, Language};
use crate::lang::set::Languages;
use crate::support_dir;

/// lexicon.json: words learned for good, and words corrected once. Both fields are required
/// so a file from another version (`{"de": "japanese"}`) fails to decode and is kept aside.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Store {
    pub learned: BTreeMap<String, Language>,
    pub once: BTreeMap<String, Language>,
}

pub struct LearningEngine {
    base: Box<dyn DecisionEngine>,
    store_path: Option<PathBuf>,
    state: Mutex<State>,
    languages: Languages,
}

struct State {
    store: Store,
    last_learned: Option<String>,
}

fn read_store(path: Option<&Path>) -> Store {
    let Some(path) = path else {
        return Store::default();
    };
    let Ok(data) = fs::read(path) else {
        return Store::default();
    };
    if let Ok(store) = serde_json::from_slice::<Store>(&data) {
        return store;
    }
    // Unreadable (hand-edited, or from another version): keep it aside, don't overwrite it.
    let mut aside = path.as_os_str().to_owned();
    aside.push(".unreadable");
    let aside = PathBuf::from(aside);
    let _ = fs::remove_file(&aside);
    let _ = fs::rename(path, &aside);
    Store::default()
}

impl LearningEngine {
    pub fn default_store_path() -> Option<PathBuf> {
        support_dir().map(|d| d.join("lexicon.json"))
    }

    pub fn new(base: Box<dyn DecisionEngine>, store_path: Option<PathBuf>) -> Self {
        let store = read_store(store_path.as_deref());
        Self {
            base,
            store_path,
            state: Mutex::new(State {
                store,
                last_learned: None,
            }),
            languages: Languages::legacy(true),
        }
    }

    /// Only these languages may come out: a word learned in a language that is now off is
    /// decided by the heuristics again.
    pub fn with_languages(mut self, languages: Languages) -> Self {
        self.languages = languages;
        self
    }

    pub fn languages(&self) -> &Languages {
        &self.languages
    }

    pub fn base(&self) -> &dyn DecisionEngine {
        self.base.as_ref()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Words learned for good (corrected the same way twice).
    pub fn learned(&self) -> BTreeMap<String, Language> {
        self.lock().store.learned.clone()
    }

    /// Words corrected once: the same correction again learns them.
    pub fn pending(&self) -> BTreeMap<String, Language> {
        self.lock().store.once.clone()
    }

    /// The keys most recently learned for good in this process, while still learned.
    pub fn last_learned(&self) -> Option<String> {
        let s = self.lock();
        s.last_learned
            .clone()
            .filter(|k| s.store.learned.contains_key(k))
    }

    /// Re-reads lexicon.json (another process or the user changed it).
    pub fn reload(&self) {
        let fresh = read_store(self.store_path.as_deref());
        self.lock().store = fresh;
    }

    /// Forgets the keys, learned or pending: the engine decides them again.
    pub fn forget(&self, keys: &str) {
        self.reload();
        let mut s = self.lock();
        if s.store.learned.remove(keys).is_some() | s.store.once.remove(keys).is_some() {
            self.persist(&s.store);
        }
    }

    /// Forgets every learned and pending word.
    pub fn forget_all(&self) {
        let mut s = self.lock();
        s.store = Store::default();
        self.persist(&s.store);
    }

    /// Records that `keys` were corrected to `language`. The second time the same correction is
    /// made it is learned; correcting back to the engine's own choice forgets the word.
    ///
    /// Pass `engine_choice` when the caller knows what the engine decided (the session always
    /// does: what was on screen, which a fresh decision might not match).
    pub fn learn(&self, keys: &str, language: Language, engine_choice: Option<Language>) {
        let base_choice = engine_choice
            .unwrap_or_else(|| self.base.decide(keys, &DecisionContext::default()).language);
        self.reload();
        let mut s = self.lock();
        // Already learned this way (e.g. converting a learned word again): nothing to change.
        if s.store.learned.get(keys) == Some(&language) {
            return;
        }
        if language == base_choice {
            s.store.learned.remove(keys);
            s.store.once.remove(keys);
        } else if s.store.learned.get(keys) == Some(&language)
            || s.store.once.get(keys) == Some(&language)
        {
            s.store.learned.insert(keys.to_string(), language);
            s.store.once.remove(keys);
            s.last_learned = Some(keys.to_string());
        } else {
            // A different correction than before: it starts over.
            s.store.learned.remove(keys);
            s.store.once.insert(keys.to_string(), language);
        }
        self.persist(&s.store);
    }

    /// Failing to save (read-only profile, sandboxed app) only costs the learning.
    fn persist(&self, store: &Store) {
        let Some(path) = &self.store_path else { return };
        let Ok(json) = serde_json::to_vec_pretty(store) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".{}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        if fs::write(&tmp, json).is_ok() && fs::rename(&tmp, path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }
}

impl DecisionEngine for LearningEngine {
    fn decide(&self, keys: &str, context: &DecisionContext) -> Decision {
        let learned = self
            .lock()
            .store
            .learned
            .get(keys)
            .copied()
            .filter(|l| self.languages.allows(*l));
        if let Some(language) = learned {
            return Decision {
                language,
                confidence: 1.0,
                output: self.base.render(keys, language),
                explanation: "learned".to_string(),
                runner_up: None,
            };
        }
        self.base.decide(keys, context)
    }

    fn render(&self, keys: &str, language: Language) -> String {
        self.base.render(keys, language)
    }
}
