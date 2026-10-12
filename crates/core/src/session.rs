//! One state machine per text client: the word being composed, the language of the previous
//! word, and the last committed word (so Esc right after a commit can change it).
//! Port of InputSession.swift, English/Korean only.
//!
//! While a word is being composed:
//!   letters        extend it; the underlined preview shows the likely language
//!   Esc            cycle it: english → english+particle (github에) → korean
//!                  (where the app's Esc is set to go to the app, Esc commits and goes to the
//!                  app; Shift+Esc cycles)
//!   Shift+Esc      cycle it, in every app
//!   Delete         in Korean the last jamo of the syllable being typed, then whole syllables
//!                  (as the system's 2-set does); else the last keystroke
//!   Space, punct.  commit the word; the app types the key itself
//!   Tab            complete the word to the user's most typed (else a built-in) word that
//!                  continues it; with no completion it commits and the app gets the Tab
//!   Return         commit the word as shown and nothing else (with `return_confirms_only`)
//!   anything else  commit and let the app handle the key
//! Right after a word was committed with space/punctuation, Esc / Shift+Esc cycle that
//! committed word the same way. The same correction made twice is learned for good.

use std::sync::{Arc, LazyLock};

use crate::engine::{Decision, DecisionContext, DecisionEngine, Language};
use crate::english::EnglishModel;
use crate::hangul::{HangulComposer, Unit};
use crate::history::{self, History};
use crate::korean::{KoreanModel, UNKNOWN_SYLLABLE_LOGP};
use crate::learning::LearningEngine;
use crate::policy::{AppPolicies, AppPolicy};

pub const NOT_FOUND: usize = usize::MAX;

/// A range in UTF-16 code units, like NSRange / ITfRangeACP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub location: usize,
    pub length: usize,
}

impl Range {
    pub fn end(&self) -> usize {
        self.location + self.length
    }
}

/// The text field the input method is typing into.
pub trait TextClient {
    fn set_marked_text(&mut self, text: &str);
    /// `replacing == None` replaces the marked text (or the selection if nothing is marked).
    fn insert_text(&mut self, text: &str, replacing: Option<Range>);
    fn selected_range(&self) -> Range;
    /// `location == NOT_FOUND` when nothing is composing.
    fn marked_range(&self) -> Range;
    fn text_in(&self, range: Range) -> Option<String>;
}

/// A client that takes nothing: for resetting state after the app already ended the composition.
pub struct NoopClient;

impl TextClient for NoopClient {
    fn set_marked_text(&mut self, _: &str) {}
    fn insert_text(&mut self, _: &str, _: Option<Range>) {}
    fn selected_range(&self) -> Range {
        Range { location: NOT_FOUND, length: 0 }
    }
    fn marked_range(&self) -> Range {
        Range { location: NOT_FOUND, length: 0 }
    }
    fn text_in(&self, _: Range) -> Option<String> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Letter(char),
    Delete,
    Escape,
    /// Shift+Esc: switch the word's language in every app, including those where Esc goes to the app.
    Cycle,
    /// Space, digit or punctuation.
    Printable(char),
    Enter,
    /// Completes the word being typed, if there is a completion.
    Tab,
    /// Arrows, shortcuts…: commit and let the app handle the key.
    Other,
}

struct Committed {
    keys: String,
    /// What the engine chose before any Esc.
    decided: Language,
    /// What it went in as (Esc after the commit changes `language`).
    committed_as: Language,
    language: Language,
    /// Exactly what was inserted: the word plus its boundary character.
    text: String,
    boundary: String,
    location: usize,
}

/// The last decision and what it was made from, for debugging.
#[derive(Clone, Debug)]
pub struct Trace {
    pub keys: String,
    pub previous: Option<Language>,
    pub decision: Decision,
    /// Still being typed (the preview) rather than committed.
    pub partial: bool,
}

struct Word {
    keys: String,
    text: String,
    language: Language,
    decided: Language,
}

/// The preview only switches language mid-word when the new reading is at least this likely.
const PREVIEW_FLIP_CONFIDENCE: f64 = 0.7;

static KOREAN: LazyLock<KoreanModel> = LazyLock::new(KoreanModel::default);

pub struct InputSession {
    engine: Arc<LearningEngine>,
    policies: AppPolicies,
    return_confirms_only: bool,
    keys: String,
    app: Option<String>,
    policy: AppPolicy,
    forced: Option<Language>,
    /// The language the engine itself showed last, before any Esc.
    engine_preview: Option<Language>,
    /// The last `decide` kept a short token (3 keys or fewer, like a command) English only
    /// because the app is set to Strict.
    kept_english_by_strict: bool,
    previous_language: Option<Language>,
    last_commit: Option<Committed>,
    paused: bool,
    preview_language: Option<Language>,
    /// The language shown when Delete was pressed, held until the next letter.
    pinned: Option<Language>,
    /// Where the Korean syllable being typed starts once Delete was used in the word.
    syllable_start: Option<usize>,
    /// The last correction handed to the learning engine: going back and forth is one correction.
    last_recorded: Option<(String, Language)>,
    last_trace: Option<Trace>,
    /// Counts committed words and completes with them; None: no autocomplete.
    history: Option<Arc<History>>,
}

fn char_count(s: &str) -> usize {
    s.chars().count()
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

impl InputSession {
    pub fn new(engine: Arc<LearningEngine>, policies: AppPolicies, return_confirms_only: bool) -> Self {
        Self {
            engine,
            policies,
            return_confirms_only,
            keys: String::new(),
            app: None,
            policy: AppPolicy::STANDARD,
            forced: None,
            engine_preview: None,
            kept_english_by_strict: false,
            previous_language: None,
            last_commit: None,
            paused: false,
            preview_language: None,
            pinned: None,
            syllable_start: None,
            last_recorded: None,
            last_trace: None,
            history: None,
        }
    }

    /// Turns on Tab completion, learning from the words committed.
    pub fn with_history(mut self, history: Arc<History>) -> Self {
        self.history = Some(history);
        self
    }

    pub fn keys(&self) -> &str {
        &self.keys
    }
    pub fn app(&self) -> Option<&str> {
        self.app.as_deref()
    }
    pub fn policy(&self) -> AppPolicy {
        self.policy
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn last_trace(&self) -> Option<&Trace> {
        self.last_trace.as_ref()
    }
    pub fn engine(&self) -> &Arc<LearningEngine> {
        &self.engine
    }
    /// A word is being composed.
    pub fn is_composing(&self) -> bool {
        !self.keys.is_empty()
    }
    /// A just-committed word can still be flipped with Esc.
    pub fn has_flip_window(&self) -> bool {
        self.last_commit.is_some()
    }

    /// A new text field became active: pick its app's policy and forget the sentence context.
    pub fn activate(&mut self, exe: Option<&str>) {
        self.close_flip_window();
        self.app = exe.map(str::to_lowercase);
        self.policy = self.policies.policy(exe);
        self.previous_language = None;
        self.paused = false;
    }

    /// Pauses or resumes in this text field; what was being typed goes in first.
    pub fn toggle_pause(&mut self, client: &mut dyn TextClient) {
        if !self.paused {
            self.commit(client);
        }
        self.paused = !self.paused;
        self.previous_language = None;
    }

    /// Returns true when the key was consumed.
    pub fn handle(&mut self, key: Key, client: &mut dyn TextClient) -> bool {
        if !self.policy.enabled || self.paused {
            return false;
        }
        let pending = self.last_commit.take();
        // Esc switches languages unless the app owns it; Shift+Esc always does.
        let switches = key == Key::Cycle || (key == Key::Escape && !self.policy.escape_to_app);
        if switches && self.keys.is_empty() {
            if let Some(p) = &pending {
                if let Some(changed) = self.cycle(p, client) {
                    self.last_commit = Some(changed);
                    return true;
                }
            }
        }
        if let Some(p) = &pending {
            self.finalize(p);
        }

        match key {
            Key::Letter(ch) => {
                if self.keys.is_empty() {
                    self.last_recorded = None; // a new word
                }
                self.pinned = None;
                self.keys.push(ch);
                self.update_preview(client);
                true
            }
            Key::Delete => {
                if self.keys.is_empty() {
                    return false;
                }
                let shown = self.forced.or(self.pinned).or(self.preview_language);
                if self.forced.is_none() {
                    self.pinned = shown;
                }
                if shown == Some(Language::Korean) {
                    self.delete_jamo_or_syllable();
                } else {
                    self.keys.pop();
                }
                if self.keys.is_empty() {
                    self.forced = None;
                    self.pinned = None;
                    self.syllable_start = None;
                }
                self.update_preview(client);
                true
            }
            Key::Escape | Key::Cycle => {
                if self.keys.is_empty() {
                    return false;
                }
                if !switches {
                    // The app's Esc: the word goes in as shown, then the app gets the key.
                    self.commit(client);
                    return false;
                }
                // Cycle from what the user sees.
                let shown = match self.forced.or(self.pinned).or(self.preview_language) {
                    Some(l) => l,
                    None => self.decide(true).language,
                };
                self.forced = Some(shown.next(&self.keys));
                self.pinned = None;
                self.update_preview(client);
                true
            }
            Key::Printable(ch) => {
                if self.keys.is_empty() {
                    return false;
                }
                let marked = client.marked_range();
                let location =
                    if marked.location != NOT_FOUND { marked.location } else { client.selected_range().location };
                let word = self.finish_word();
                // Only the word goes in; the app types the space or punctuation itself, as with
                // the system's Korean input.
                client.insert_text(&word.text, None);
                self.last_commit = Some(Committed {
                    keys: word.keys,
                    decided: word.decided,
                    committed_as: word.language,
                    language: word.language,
                    text: format!("{}{}", word.text, ch),
                    boundary: ch.to_string(),
                    location,
                });
                false
            }
            Key::Enter => {
                if self.keys.is_empty() {
                    return false;
                }
                // Like a Japanese IME, Return on an underlined word only confirms it; the next
                // Return is the app's. With the setting off the app gets this one too.
                self.commit(client);
                self.return_confirms_only
            }
            Key::Tab => {
                let Some(word) = self.suggestion() else {
                    self.commit(client);
                    self.previous_language = None;
                    return false;
                };
                let language = self.shown_language().unwrap_or(Language::English);
                self.reset_word();
                client.insert_text(&word, None);
                if let Some(h) = &self.history {
                    h.record(&word);
                }
                self.previous_language = Some(language.sentence_language());
                true
            }
            Key::Other => {
                self.commit(client);
                self.previous_language = None;
                false
            }
        }
    }

    /// Commits whatever is in progress as-is (focus change, shortcut, deactivation).
    pub fn commit(&mut self, client: &mut dyn TextClient) {
        self.close_flip_window();
        if self.keys.is_empty() {
            return;
        }
        let word = self.finish_word();
        client.insert_text(&word.text, None);
    }

    /// Commits what's in progress after the caret was moved elsewhere (a click): the next
    /// word doesn't continue the previous sentence.
    pub fn commit_for_new_context(&mut self, client: &mut dyn TextClient) {
        self.commit(client);
        if let Some(h) = &self.history {
            h.flush();
        }
        self.previous_language = None;
    }

    // MARK: - Private

    /// The language on screen for the word being typed.
    fn shown_language(&self) -> Option<Language> {
        self.forced.or(self.pinned).or(self.preview_language)
    }

    /// The word Tab would complete the one being typed to.
    fn suggestion(&self) -> Option<String> {
        let language = self.shown_language()?;
        let prefix = self.preview(language)?;
        history::suggest(self.history.as_deref(), language, &prefix)
    }

    fn decide(&mut self, partial: bool) -> Decision {
        let ctx = DecisionContext { previous_language: self.previous_language, partial };
        let d = self.engine.decide(&self.keys, &ctx);
        let threshold = self.policy.conversion_threshold;
        let kept = d.language != Language::English && d.confidence < threshold;
        self.kept_english_by_strict =
            kept && threshold >= AppPolicy::STRICT_THRESHOLD && char_count(&self.keys) <= 3;
        let decision = if kept {
            Decision {
                language: Language::English,
                confidence: 1.0 - d.confidence,
                output: self.keys.clone(),
                explanation: format!("kept English ({:.2} < {:.2}); {}", d.confidence, threshold, d.explanation),
                runner_up: d.runner_up,
            }
        } else {
            d
        };
        self.last_trace = Some(Trace {
            keys: self.keys.clone(),
            previous: self.previous_language,
            decision: decision.clone(),
            partial,
        });
        decision
    }

    /// What Space/Return commits without Esc: the full-word decision, except that a hesitant
    /// change away from the reading on screen doesn't win. The same rule as the preview, so
    /// what you see is what you get.
    fn committed_language(&mut self) -> Language {
        let d = self.decide(false);
        match self.engine_preview {
            Some(shown) if shown != d.language && !self.is_weak_flip_allowed(&d, shown) => shown,
            _ => d.language,
        }
    }

    /// Whether `decision` may replace the reading on screen: when it's confident, when it's a
    /// dictionary English word (which is what Space/Return will commit), or when a Strict app
    /// kept a short token English.
    fn is_weak_flip_allowed(&self, decision: &Decision, shown: Language) -> bool {
        decision.confidence >= PREVIEW_FLIP_CONFIDENCE
            || (decision.language == Language::English
                && (self.kept_english_by_strict || EnglishModel::shared().is_known_word(&self.keys)))
            || self.preview(shown).is_none()
    }

    fn update_preview(&mut self, client: &mut dyn TextClient) {
        if self.keys.is_empty() {
            self.preview_language = None;
            self.engine_preview = None;
            client.set_marked_text("");
        } else if let Some(forced) = self.forced.or(self.pinned) {
            self.preview_language = Some(forced);
            let text = match forced {
                Language::Korean => self.hangul().unwrap_or_else(|| self.keys.clone()),
                other => self.engine.render(&self.keys, other),
            };
            client.set_marked_text(&text);
        } else {
            self.split_off_korean_before_other(client);
            let decision = self.decide(true);
            let mut language = decision.language;
            // Hysteresis: a weak flip on one keystroke isn't shown, except into a dictionary
            // English word, which is what Space/Return will commit.
            if let Some(shown) = self.preview_language {
                if shown != language && char_count(&self.keys) > 1 && !self.is_weak_flip_allowed(&decision, shown) {
                    language = shown;
                }
            }
            self.preview_language = Some(language);
            self.engine_preview = Some(language);
            let text = self.preview(language).unwrap_or_else(|| self.keys.clone());
            client.set_marked_text(&text);
        }
    }

    /// Korean with English typed straight after it reads as broken Hangul: 오늘은 then
    /// google에서 (dhsmfdmsgoogledptj). When the keys only make Hangul with stray jamo, but a
    /// front of two or more ordinary syllables is followed by a known English word (or one with
    /// a particle), the Korean goes in as typed and the rest carries on as its own word.
    fn split_off_korean_before_other(&mut self, client: &mut dyn TextClient) {
        let chars: Vec<char> = self.keys.chars().collect();
        if chars.len() < 7 {
            return;
        }
        if HangulComposer::compose(&self.keys).is_none_or(|c| !c.is_broken()) {
            return;
        }
        // Only what reads as Korean on its own, judged without context.
        let alone = self.engine.decide(&self.keys, &DecisionContext { previous_language: None, partial: true });
        if alone.language != Language::Korean {
            return;
        }
        for cut in 4..=(chars.len() - 3) {
            let front: String = chars[..cut].iter().collect();
            let back: String = chars[cut..].iter().collect();
            let Some(korean) = HangulComposer::compose(&front) else { continue };
            if !korean.stray_jamo().is_empty()
                || korean.syllables().len() < 2
                || !korean.syllables().iter().all(|s| KOREAN.syllable_log_probability(*s) > UNKNOWN_SYLLABLE_LOGP)
                || !self.is_other_language_word(&back)
            {
                continue;
            }
            client.insert_text(&korean.text(), None);
            self.keys = back;
            self.syllable_start = None;
            self.previous_language = Some(Language::Korean);
            self.preview_language = None;
            self.engine_preview = None;
            return;
        }
    }

    /// A known English word, or one with a Korean particle (google에서).
    fn is_other_language_word(&self, keys: &str) -> bool {
        let ctx = DecisionContext { previous_language: Some(Language::Korean), partial: true };
        match self.engine.decide(keys, &ctx).language {
            Language::English => char_count(keys) >= 3 && EnglishModel::shared().is_known_word(keys),
            Language::Mixed => true,
            Language::Korean => false,
        }
    }

    /// The word so far rendered in `language`, or None when it has no such reading.
    fn preview(&self, language: Language) -> Option<String> {
        match language {
            Language::English => Some(self.keys.clone()),
            Language::Korean => self.hangul(),
            Language::Mixed => {
                let text = self.engine.render(&self.keys, Language::Mixed);
                (text != self.keys).then_some(text)
            }
        }
    }

    /// Decides the current word, learns an Esc override, and resets the buffer.
    fn finish_word(&mut self) -> Word {
        let engine_choice = self.committed_language();
        let language = self.forced.or(self.pinned).unwrap_or(engine_choice);
        // A reading kept while deleting isn't a correction: report it as the engine's.
        let decided = if self.forced.is_none() && self.pinned.is_some() { language } else { engine_choice };
        if self.forced.is_some() {
            self.record(&self.keys.clone(), language, decided);
        }
        let text = match language {
            Language::Korean => self.hangul().unwrap_or_else(|| self.engine.render(&self.keys, language)),
            other => self.engine.render(&self.keys, other),
        };
        if let Some(h) = &self.history {
            h.record(&text);
        }
        let word = Word { keys: std::mem::take(&mut self.keys), text, language, decided };
        self.previous_language = Some(language.sentence_language());
        self.reset_word();
        word
    }

    /// Starts a new word: the caller has taken the keys or no longer needs them.
    fn reset_word(&mut self) {
        self.keys.clear();
        self.forced = None;
        self.pinned = None;
        self.syllable_start = None;
        self.preview_language = None;
        self.engine_preview = None;
    }

    /// Korean as the system's 2-set input deletes it: jamo by jamo in the syllable being typed
    /// (한국 → 한구 → 한ㄱ → 한), then whole syllables, which it had already committed.
    fn delete_jamo_or_syllable(&mut self) {
        let chars: Vec<char> = self.keys.chars().collect();
        let Some(composed) = HangulComposer::compose(&self.keys) else {
            self.keys.pop();
            return;
        };
        let earlier = &composed.units[..composed.units.len().saturating_sub(1)];
        let start = *self.syllable_start.get_or_insert_with(|| Self::cut(&chars, earlier));
        if start < chars.len() {
            self.keys.pop();
            return;
        }
        // Nothing left being typed: a whole committed syllable goes.
        let cut = Self::cut(&chars, earlier);
        self.keys = chars[..cut].iter().collect();
        self.syllable_start = Some(cut);
    }

    /// Where the keys of the last syllable start: the longest front that makes `earlier` alone.
    fn cut(chars: &[char], earlier: &[Unit]) -> usize {
        (0..chars.len())
            .rev()
            .find(|&i| {
                let front: String = chars[..i].iter().collect();
                HangulComposer::compose(&front).is_some_and(|c| c.units == earlier)
            })
            .unwrap_or(chars.len().saturating_sub(1))
    }

    /// The word as Hangul, with the syllable being typed kept apart from those before it
    /// after a Delete (`syllable_start`).
    fn hangul(&self) -> Option<String> {
        let chars: Vec<char> = self.keys.chars().collect();
        match self.syllable_start {
            Some(start) if start < chars.len() => {
                let front: String = chars[..start].iter().collect();
                let back: String = chars[start..].iter().collect();
                Some(HangulComposer::compose(&front)?.text() + &HangulComposer::compose(&back)?.text())
            }
            _ => HangulComposer::compose(&self.keys).map(|c| c.text()),
        }
    }

    fn record(&mut self, keys: &str, language: Language, engine_choice: Language) {
        if self.last_recorded.as_ref().is_some_and(|(k, l)| k == keys && *l == language) {
            return;
        }
        self.last_recorded = Some((keys.to_string(), language));
        self.engine.learn(keys, language, Some(engine_choice));
    }

    /// Replaces a just-committed word with its next rendering, but only if the caret still
    /// sits right after it and the text there is what we inserted.
    fn cycle(&mut self, commit: &Committed, client: &mut dyn TextClient) -> Option<Committed> {
        let range = Range { location: commit.location, length: utf16_len(&commit.text) };
        let cursor = client.selected_range();
        if cursor.location == NOT_FOUND
            || cursor.length != 0
            || cursor.location != range.end()
            || client.text_in(range).as_deref() != Some(commit.text.as_str())
        {
            return None;
        }
        let language = commit.language.next(&commit.keys);
        let text = format!("{}{}", self.engine.render(&commit.keys, language), commit.boundary);
        client.insert_text(&text, Some(range));
        self.previous_language = Some(language.sentence_language());
        Some(Committed {
            keys: commit.keys.clone(),
            decided: commit.decided,
            committed_as: commit.committed_as,
            language,
            text,
            boundary: commit.boundary.clone(),
            location: commit.location,
        })
    }

    fn close_flip_window(&mut self) {
        if let Some(pending) = self.last_commit.take() {
            self.finalize(&pending);
        }
    }

    /// The undo window closed: learn where the word ended up (once, not every reading Esc
    /// passed through on the way).
    fn finalize(&mut self, commit: &Committed) {
        if commit.language != commit.committed_as {
            self.record(&commit.keys, commit.language, commit.decided);
        }
    }
}
