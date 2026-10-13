//! The undo window after a word is committed: Esc re-renders it, and what it ends as is learned.

use super::{utf16_len, InputSession, Range, TextClient, NOT_FOUND};
use crate::decide::engine::{DecisionEngine, Language};
use crate::decide::segmenter::JapaneseSegmenter;
use crate::lang::set::Languages;

pub(super) struct Committed {
    pub(super) keys: String,
    /// What the engine chose before any Esc.
    pub(super) decided: Language,
    /// What it went in as (Esc after the commit changes `language`).
    pub(super) committed_as: Language,
    pub(super) language: Language,
    /// Exactly what was inserted: the word plus its boundary character.
    pub(super) text: String,
    pub(super) boundary: String,
    pub(super) location: usize,
}

impl InputSession {
    /// Replaces a just-committed word with its next rendering, but only if the caret still
    /// sits right after it and the text there is what we inserted.
    pub(super) fn cycle(
        &mut self,
        commit: &Committed,
        client: &mut dyn TextClient,
    ) -> Option<Committed> {
        let range = Range {
            location: commit.location,
            length: utf16_len(&commit.text),
        };
        let cursor = client.selected_range();
        if cursor.location == NOT_FOUND
            || cursor.length != 0
            || cursor.location != range.end()
            || client.text_in(range).as_deref() != Some(commit.text.as_str())
        {
            return None;
        }
        let language = commit
            .language
            .next(&commit.keys, &Languages::legacy(self.japanese_enabled));
        let reading = if language == Language::Japanese {
            JapaneseSegmenter::reading_of(&commit.keys, self.segmenter.as_ref())
        } else {
            self.engine.render(&commit.keys, language)
        };
        let text = format!("{reading}{}", commit.boundary);
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

    pub(super) fn close_flip_window(&mut self) {
        if let Some(pending) = self.last_commit.take() {
            self.finalize(&pending);
        }
    }

    /// The undo window closed: learn where the word ended up (once, not every reading Esc
    /// passed through on the way).
    pub(super) fn finalize(&mut self, commit: &Committed) {
        if commit.language != commit.committed_as {
            self.record(&commit.keys, commit.language, commit.decided);
        }
    }

    pub(super) fn record(&mut self, keys: &str, language: Language, engine_choice: Language) {
        if self
            .last_recorded
            .as_ref()
            .is_some_and(|(k, l)| k == keys && *l == language)
        {
            return;
        }
        self.last_recorded = Some((keys.to_string(), language));
        self.engine.learn(keys, language, Some(engine_choice));
    }
}
