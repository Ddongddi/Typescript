//! Words the user corrected with Esc (LearningEngine in DecisionEngine.swift): a correction
//! made twice wins over the heuristics. Stored in `%APPDATA%\tacit\lexicon.json`.
//!
//! Several processes share the file (a TIP lives inside every app), so changes re-read the
//! file first and replace it atomically, with a file lock held through the transaction.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::decide::engine::CONFIDENCE_SLOPE;
use crate::decide::engine::{Decision, DecisionContext, DecisionEngine, Language};
use crate::decide::mixed::MixedWord;
use crate::decide::shape::shape_of;
use crate::lang::hangul::HangulComposer;
use crate::lang::registry;
use crate::lang::set::Languages;
use crate::support_dir;

/// A stem shorter than this many keys isn't learned on its own (go, so, 하): the short words
/// are the ambiguous ones, they stay word by word.
const MIN_STEM_KEYS: usize = 3;

/// How far one corrected word of a kind moves the engine's margin toward the language it was
/// corrected to, and the most a kind can move it: a coin flip follows one correction, a word
/// the engine is fairly sure of takes several.
const SHAPE_STEP: f64 = 0.4;
const SHAPE_MAX_BIAS: f64 = 1.6;
/// A flip by the kind never reads as surer than this.
const SHAPE_MAX_CONFIDENCE: f64 = 0.75;

/// The learned language of a stem that `keys` continue with a particle or ending, and where
/// the ending starts. Looks the prefixes up in the map first (cheap), and only checks the
/// ending of the ones that are there.
fn learned_stem(learned: &BTreeMap<String, Language>, keys: &str) -> Option<(Language, usize)> {
    if learned.is_empty() {
        return None;
    }
    keys.char_indices()
        .skip(MIN_STEM_KEYS)
        .filter_map(|(i, _)| Some((*learned.get(&keys[..i])?, i)))
        .find(|(_, cut)| MixedWord::is_ending_keys(&keys[*cut..]))
}

/// The stem of `keys` when a particle or ending is typed straight after it: fltpssmrk → fltpssm.
fn stem_of(keys: &str) -> Option<String> {
    MixedWord::split(keys)
        .map(|m| m.english)
        .filter(|stem| stem.chars().count() >= MIN_STEM_KEYS)
}

/// lexicon.json: words learned for good, and words corrected once. Both fields are required
/// so a file from another version (`{"de": "japanese"}`) fails to decode and is kept aside.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Store {
    pub learned: BTreeMap<String, Language>,
    pub once: BTreeMap<String, Language>,
    /// How many different words of a kind were corrected to a language: `"jamo:korean"`.
    /// Optional, so a file from before kinds were learned still reads (an older program
    /// ignores the field).
    #[serde(default)]
    pub shapes: BTreeMap<String, u32>,
}

fn shape_key(shape: &str, language: Language) -> String {
    format!("{shape}:{}", language.name())
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
    crate::store::load_or_quarantine(path)
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
            languages: Languages::legacy(false),
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
        let mut state = self.lock();
        state.store = read_store(self.store_path.as_deref());
    }

    fn change(&self, change: impl FnOnce(&mut State) -> bool) {
        let mut state = self.lock();
        let file_lock = self
            .store_path
            .as_deref()
            .map(crate::store::lock_file)
            .transpose();
        // No store path keeps the existing stateless learning behavior.
        if file_lock.is_ok() {
            state.store = read_store(self.store_path.as_deref());
        }
        // A read-only profile still learns in memory, but never writes without the lock.
        if change(&mut state) && file_lock.is_ok() {
            self.persist(&state.store);
        }
    }

    /// Forgets the keys, learned or pending: the engine decides them again.
    pub fn forget(&self, keys: &str) {
        self.change(|s| {
            s.store.learned.remove(keys).is_some() | s.store.once.remove(keys).is_some()
        });
    }

    /// Forgets every learned and pending word.
    pub fn forget_all(&self) {
        self.change(|s| {
            s.store = Store::default();
            true
        });
    }

    /// Records that `keys` were corrected to `language`. The second time the same correction is
    /// made it is learned; correcting back to the engine's own choice forgets the word.
    ///
    /// Pass `engine_choice` when the caller knows what the engine decided (the session always
    /// does: what was on screen, which a fresh decision might not match).
    pub fn learn(&self, keys: &str, language: Language, engine_choice: Option<Language>) {
        let base_choice = engine_choice
            .unwrap_or_else(|| self.base.decide(keys, &DecisionContext::default()).language);
        self.change(|s| {
            // Already learned this way (e.g. converting a learned word again): nothing to change.
            if s.store.learned.get(keys) == Some(&language) {
                return false;
            }
            // A new word corrected this way is evidence for its kind of word, once.
            if language != base_choice
                && language != Language::Japanese
                && s.store.once.get(keys) != Some(&language)
            {
                let count = s
                    .store
                    .shapes
                    .entry(shape_key(shape_of(keys), language))
                    .or_insert(0);
                *count = count.saturating_add(1);
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
                // English + ending corrected to Mixed teaches the English stem (kubectl); a Korean
                // correction teaches the Korean one. Then every other ending on it follows.
                let stem_language = match language {
                    Language::Mixed => Some(Language::English),
                    Language::Korean => Some(Language::Korean),
                    _ => None,
                };
                if let (Some(stem), Some(l)) = (stem_of(keys), stem_language) {
                    s.store.learned.entry(stem).or_insert(l);
                }
            } else {
                // A different correction than before: it starts over.
                s.store.learned.remove(keys);
                s.store.once.insert(keys.to_string(), language);
            }
            true
        });
    }

    /// Failing to save (read-only profile, sandboxed app) only costs the learning.
    fn persist(&self, store: &Store) {
        if let Some(path) = &self.store_path {
            let _ = crate::store::persist(path, store);
        }
    }
}

impl LearningEngine {
    /// The engine's own pick, unless the corrections made to words of this kind weigh more than
    /// how sure the engine was: a coin flip follows one correction, a sure word stays. Only
    /// between English and Korean (the English + Korean cut has its own stem learning: moving
    /// words into it by kind broke plain English words in the lab), and only where the keys
    /// read in the other language.
    fn by_shape(&self, keys: &str, base: Decision) -> Decision {
        let sure = base.confidence >= 1.0 - f64::EPSILON;
        if sure || !matches!(base.language, Language::English | Language::Korean) {
            return base;
        }
        // Nothing corrected yet (most users, most of the time): no cost.
        let shapes = self.lock().store.shapes.clone();
        if shapes.is_empty() {
            return base;
        }
        let shape = shape_of(keys);
        let evidence = |l: Language| shapes.get(&shape_key(shape, l)).copied().unwrap_or(0);
        let against = evidence(base.language);
        let Some((language, bias)) = [Language::English, Language::Korean]
            .into_iter()
            .filter(|l| *l != base.language && self.languages.allows(*l))
            .filter_map(|l| {
                let net = f64::from(evidence(l)) - f64::from(against);
                (net > 0.0).then(|| (l, (net * SHAPE_STEP).min(SHAPE_MAX_BIAS)))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
        else {
            return base;
        };
        let margin = (base.confidence / (1.0 - base.confidence)).ln() / CONFIDENCE_SLOPE;
        if margin >= bias || registry::reading(language, keys).is_none() {
            return base;
        }
        let confidence = 1.0 / (1.0 + (-(bias - margin) * CONFIDENCE_SLOPE).exp());
        Decision {
            language,
            confidence: confidence.min(SHAPE_MAX_CONFIDENCE),
            output: self.base.render(keys, language),
            explanation: format!("learned kind {shape}: {}", base.explanation),
            runner_up: Some(base.language),
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
        // The stem's language and where its ending starts.
        let stem = || {
            let (language, cut) = learned_stem(&self.lock().store.learned, keys)?;
            match language {
                Language::English if self.languages.allows(Language::Korean) => {
                    Some((Language::Mixed, Some(cut)))
                }
                Language::Korean if self.languages.allows(Language::Korean) => {
                    Some((Language::Korean, None))
                }
                _ => None,
            }
        };
        if let Some((language, cut, why)) = learned
            .map(|l| (l, None, "learned"))
            .or_else(|| stem().map(|(l, cut)| (l, cut, "learned stem")))
        {
            // An English stem keeps its own cut: the ending is read from where the stem ends.
            let output = match (language, cut) {
                (Language::Mixed, Some(cut)) if why == "learned stem" => {
                    HangulComposer::compose(&keys[cut..]).map_or_else(
                        || self.base.render(keys, language),
                        |ending| format!("{}{}", &keys[..cut], ending.text()),
                    )
                }
                _ => self.base.render(keys, language),
            };
            return Decision {
                language,
                confidence: 1.0,
                output,
                explanation: why.to_string(),
                runner_up: None,
            };
        }
        self.by_shape(keys, self.base.decide(keys, context))
    }

    fn render(&self, keys: &str, language: Language) -> String {
        self.base.render(keys, language)
    }
}
