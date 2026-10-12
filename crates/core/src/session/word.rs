//! The word being typed: its keys and how the user has steered its reading.

use crate::decide::engine::Language;
use crate::lang::hangul::{HangulComposer, Unit};

#[derive(Default)]
pub(super) struct WordState {
    pub keys: String,
    /// The reading chosen with Esc.
    pub forced: Option<Language>,
    /// The language shown when Delete was pressed, held until the next letter.
    pub pinned: Option<Language>,
    /// The reading currently on screen.
    pub preview_language: Option<Language>,
    /// The language the engine itself showed last, before any Esc.
    pub engine_preview: Option<Language>,
    /// Where the Korean syllable being typed starts once Delete was used in the word.
    pub syllable_start: Option<usize>,
}

impl WordState {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// A reading the user chose or kept, which the engine doesn't get to change.
    pub fn override_language(&self) -> Option<Language> {
        self.forced.or(self.pinned)
    }

    /// The language on screen for the word being typed.
    pub fn shown_language(&self) -> Option<Language> {
        self.override_language().or(self.preview_language)
    }

    /// Starts a new word: the caller has taken the keys or no longer needs them.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Korean as the system's 2-set input deletes it: jamo by jamo in the syllable being typed
    /// (한국 → 한구 → 한ㄱ → 한), then whole syllables, which it had already committed.
    pub(super) fn delete_jamo_or_syllable(&mut self) {
        let chars: Vec<char> = self.keys.chars().collect();
        let Some(composed) = HangulComposer::compose(&self.keys) else {
            self.keys.pop();
            return;
        };
        let earlier = &composed.units[..composed.units.len().saturating_sub(1)];
        let start = *self
            .syllable_start
            .get_or_insert_with(|| Self::cut(&chars, earlier));
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
    pub(super) fn hangul(&self) -> Option<String> {
        let chars: Vec<char> = self.keys.chars().collect();
        match self.syllable_start {
            Some(start) if start < chars.len() => {
                let front: String = chars[..start].iter().collect();
                let back: String = chars[start..].iter().collect();
                Some(
                    HangulComposer::compose(&front)?.text()
                        + &HangulComposer::compose(&back)?.text(),
                )
            }
            _ => HangulComposer::compose(&self.keys).map(|c| c.text()),
        }
    }
}
