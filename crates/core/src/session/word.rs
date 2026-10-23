//! The word being typed: its keys and how the user has steered its reading.

use crate::decide::engine::Language;
use crate::lang::hangul::{HangulComposer, Unit};

#[derive(Default)]
pub(super) struct WordState {
    pub keys: String,
    /// Clause context retained while a remaining Japanese reading is edited.
    #[cfg(feature = "japanese")]
    pub previous_clause: Option<crate::decide::convert::ConversionCandidate>,
    // The reading's steering is private: it changes only through the transitions below, so
    // `forced_by_user` can never outlive `forced`.
    /// The reading chosen with Esc.
    forced: Option<Language>,
    /// Whether `forced` is the user's choice (Esc) rather than "back to kana" after converting
    /// a phrase the engine read as Japanese anyway: only the user's choices are corrections.
    forced_by_user: bool,
    /// The language shown when Delete was pressed, held until the next letter.
    pinned: Option<Language>,
    /// The reading currently on screen.
    preview_language: Option<Language>,
    /// The language the engine itself showed last, before any Esc.
    engine_preview: Option<Language>,
    /// Partial first-key hypothesis, used only when the next letter starts a longer word.
    /// The standalone reading on screen remains authoritative for confirmation and editing.
    continuation_language: Option<Language>,
    /// Where the Korean syllable being typed starts once Delete was used in the word.
    pub syllable_start: Option<usize>,
}

impl WordState {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn forced(&self) -> Option<Language> {
        self.forced
    }
    /// The forced reading is the user's own choice (Esc), not "back to kana" after a conversion.
    pub fn forced_by_user(&self) -> bool {
        self.forced.is_some() && self.forced_by_user
    }
    pub fn pinned(&self) -> Option<Language> {
        self.pinned
    }
    pub fn preview_language(&self) -> Option<Language> {
        self.preview_language
    }
    pub fn engine_preview(&self) -> Option<Language> {
        self.engine_preview
    }
    pub fn seed_continuation(&mut self, language: Language) {
        self.continuation_language = Some(language);
    }
    pub fn take_continuation(&mut self) -> Option<Language> {
        self.continuation_language.take()
    }

    /// The reading is fixed to `language` (by the user's Esc, or by the session).
    pub fn force(&mut self, language: Language, by_user: bool) {
        self.continuation_language = None;
        self.forced = Some(language);
        self.forced_by_user = by_user;
    }
    /// Keeps the language shown when Delete was pressed, until the next letter.
    pub fn pin(&mut self, shown: Option<Language>) {
        self.continuation_language = None;
        self.pinned = shown;
    }
    pub fn unpin(&mut self) {
        self.pinned = None;
    }
    /// Shows `language` on screen as a fixed reading (the engine did not choose it).
    pub fn show(&mut self, language: Language) {
        self.preview_language = Some(language);
    }
    /// Shows `language`, which the engine itself picked.
    pub fn show_engine(&mut self, language: Language) {
        self.preview_language = Some(language);
        self.engine_preview = Some(language);
    }
    /// Nothing is on screen for this word any more.
    pub fn forget_preview(&mut self) {
        self.continuation_language = None;
        self.preview_language = None;
        self.engine_preview = None;
    }
    /// The word became a Japanese phrase being converted: no steering is left over.
    #[cfg(feature = "japanese")]
    pub fn release(&mut self) {
        self.continuation_language = None;
        self.forced = None;
        self.forced_by_user = false;
        self.pinned = None;
        self.engine_preview = None;
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

    /// After a Delete: the keys typed since (the syllable being mended) still make Hangul, apart
    /// from the jamo being typed now.
    pub(super) fn edit_reads_as_hangul(&self) -> bool {
        let start = self.syllable_start.unwrap_or(0);
        let back: String = self.keys.chars().skip(start).collect();
        HangulComposer::compose(&back).is_some_and(|c| {
            c.units
                .split_last()
                .is_some_and(|(_, front)| front.iter().all(|u| matches!(u, Unit::Syllable(_))))
        })
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
