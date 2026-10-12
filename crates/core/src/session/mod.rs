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

mod client;
mod flip;
mod key;
mod preview;
pub mod testing;
mod word;

pub use client::{NoopClient, Range, TextClient, NOT_FOUND};
use flip::Committed;
pub use key::Key;
pub use preview::Trace;
use word::WordState;

use std::sync::Arc;

use crate::decide::engine::{DecisionEngine, Language};
use crate::decide::learning::LearningEngine;
use crate::store::history::{self, History};
use crate::store::policy::{AppPolicies, AppPolicy};

struct Word {
    keys: String,
    text: String,
    language: Language,
    decided: Language,
}

pub struct InputSession {
    engine: Arc<LearningEngine>,
    policies: AppPolicies,
    return_confirms_only: bool,
    word: WordState,
    app: Option<String>,
    policy: AppPolicy,
    /// The last `decide` kept a short token (3 keys or fewer, like a command) English only
    /// because the app is set to Strict.
    kept_english_by_strict: bool,
    previous_language: Option<Language>,
    last_commit: Option<Committed>,
    paused: bool,
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
    pub fn new(
        engine: Arc<LearningEngine>,
        policies: AppPolicies,
        return_confirms_only: bool,
    ) -> Self {
        Self {
            engine,
            policies,
            return_confirms_only,
            word: WordState::default(),
            app: None,
            policy: AppPolicy::STANDARD,
            kept_english_by_strict: false,
            previous_language: None,
            last_commit: None,
            paused: false,
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
        &self.word.keys
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
        !self.word.is_empty()
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
        if switches && self.word.is_empty() {
            if let Some(changed) = pending.as_ref().and_then(|p| self.cycle(p, client)) {
                self.last_commit = Some(changed);
                return true;
            }
        }
        if let Some(p) = &pending {
            self.finalize(p);
        }

        match key {
            Key::Letter(ch) => self.on_letter(ch, client),
            Key::Delete => self.on_delete(client),
            Key::Escape | Key::Cycle => self.on_cycle(switches, client),
            Key::Printable(ch) => self.on_printable(ch, client),
            Key::Enter => self.on_enter(client),
            Key::Tab => self.on_tab(client),
            Key::Other => self.on_other(client),
        }
    }

    fn on_letter(&mut self, ch: char, client: &mut dyn TextClient) -> bool {
        if self.word.is_empty() {
            self.last_recorded = None; // a new word
        }
        self.word.pinned = None;
        self.word.keys.push(ch);
        self.update_preview(client);
        true
    }

    fn on_delete(&mut self, client: &mut dyn TextClient) -> bool {
        if self.word.is_empty() {
            return false;
        }
        let shown = self.word.shown_language();
        if self.word.forced.is_none() {
            self.word.pinned = shown;
        }
        if shown == Some(Language::Korean) {
            self.word.delete_jamo_or_syllable();
        } else {
            self.word.keys.pop();
        }
        if self.word.is_empty() {
            self.word.reset();
        }
        self.update_preview(client);
        true
    }

    /// Esc / Shift+Esc on a word being typed. `switches`: false where the app owns Esc.
    fn on_cycle(&mut self, switches: bool, client: &mut dyn TextClient) -> bool {
        if self.word.is_empty() {
            return false;
        }
        if !switches {
            // The app's Esc: the word goes in as shown, then the app gets the key.
            self.commit(client);
            return false;
        }
        // Cycle from what the user sees.
        let shown = match self.word.shown_language() {
            Some(l) => l,
            None => self.decide(true).language,
        };
        self.word.forced = Some(shown.next(&self.word.keys));
        self.word.pinned = None;
        self.update_preview(client);
        true
    }

    /// Space, digit or punctuation: the word goes in, the app types the key itself.
    fn on_printable(&mut self, ch: char, client: &mut dyn TextClient) -> bool {
        if self.word.is_empty() {
            return false;
        }
        let marked = client.marked_range();
        let location = if marked.location != NOT_FOUND {
            marked.location
        } else {
            client.selected_range().location
        };
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

    fn on_enter(&mut self, client: &mut dyn TextClient) -> bool {
        if self.word.is_empty() {
            return false;
        }
        // Like a Japanese IME, Return on an underlined word only confirms it; the next
        // Return is the app's. With the setting off the app gets this one too.
        self.commit(client);
        self.return_confirms_only
    }

    fn on_tab(&mut self, client: &mut dyn TextClient) -> bool {
        let Some(word) = self.suggestion() else {
            return self.on_other(client);
        };
        let language = self.word.shown_language().unwrap_or(Language::English);
        self.word.reset();
        client.insert_text(&word, None);
        if let Some(h) = &self.history {
            h.record(&word);
        }
        self.previous_language = Some(language.sentence_language());
        true
    }

    /// Arrows, shortcuts…: what is typed goes in and the app handles the key.
    fn on_other(&mut self, client: &mut dyn TextClient) -> bool {
        self.commit(client);
        self.previous_language = None;
        false
    }

    /// Commits whatever is in progress as-is (focus change, shortcut, deactivation).
    pub fn commit(&mut self, client: &mut dyn TextClient) {
        self.close_flip_window();
        if self.word.is_empty() {
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
            h.reload();
        }
        self.previous_language = None;
    }

    // MARK: - Private

    /// The word Tab would complete the one being typed to (None: Tab passes through, and
    /// always without autocomplete, i.e. without a history).
    pub fn suggestion(&self) -> Option<String> {
        let history = self.history.as_deref()?;
        let language = self.word.shown_language()?;
        let prefix = self.preview(language)?;
        history::suggest(Some(history), language, &prefix)
    }

    /// Decides the current word, learns an Esc override, and resets the buffer.
    fn finish_word(&mut self) -> Word {
        let engine_choice = self.committed_language();
        let language = self.word.override_language().unwrap_or(engine_choice);
        // A reading kept while deleting isn't a correction: report it as the engine's.
        let decided = if self.word.forced.is_none() && self.word.pinned.is_some() {
            language
        } else {
            engine_choice
        };
        if self.word.forced.is_some() {
            self.record(&self.word.keys.clone(), language, decided);
        }
        let text = match language {
            Language::Korean => self
                .word
                .hangul()
                .unwrap_or_else(|| self.engine.render(&self.word.keys, language)),
            other => self.engine.render(&self.word.keys, other),
        };
        if let Some(h) = &self.history {
            h.record(&text);
        }
        let word = Word {
            keys: std::mem::take(&mut self.word.keys),
            text,
            language,
            decided,
        };
        self.previous_language = Some(language.sentence_language());
        self.word.reset();
        word
    }
}
