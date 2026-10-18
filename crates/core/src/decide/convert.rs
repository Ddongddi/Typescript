//! Kana → kanji conversion (JapaneseConverter.h in the original: `AzooKeyConverter` in the
//! app, `AppleJapaneseConverter` where macOS's engine can be loaded, fakes in tests).
//!
//! DictionaryConverter implements this interface with a native Rust lattice and absorbed Mozc
//! dictionary/cost data. KanaConverter supplies the reading-only fallback used in tests.

use crate::lang::romaji::RomajiComposer;

/// One conversion candidate for (a prefix of) the hiragana being converted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversionCandidate {
    pub text: String,
    /// How many kana from the start this candidate covers. Less than the whole
    /// reading means choosing it converts the first clause and keeps converting the rest.
    pub kana_count: usize,
    /// A completion of the reading (ゆーちゅ → YouTube) rather than a conversion of it: offered
    /// in the candidate list, never shown by live conversion.
    pub is_prediction: bool,
    /// Dictionary right-context ID of the selected path; absent without a dictionary path.
    pub(crate) right_context: Option<u16>,
}

impl ConversionCandidate {
    pub fn new(text: impl Into<String>, kana_count: usize) -> Self {
        Self {
            text: text.into(),
            kana_count,
            is_prediction: false,
            right_context: None,
        }
    }

    pub fn prediction(text: impl Into<String>, kana_count: usize) -> Self {
        Self {
            text: text.into(),
            kana_count,
            is_prediction: true,
            right_context: None,
        }
    }
}

/// Kana → kanji conversion.
pub trait JapaneseConverter: Send + Sync {
    fn candidates(&self, kana: &str) -> Vec<ConversionCandidate>;
    /// Continue after a committed clause. Converters without path metadata use BOS.
    fn candidates_after(
        &self,
        kana: &str,
        _previous: &ConversionCandidate,
    ) -> Vec<ConversionCandidate> {
        self.candidates(kana)
    }
    /// First whole-reading conversion, with the same ordering as the candidate list.
    fn best_candidate(
        &self,
        kana: &str,
        previous: Option<&ConversionCandidate>,
    ) -> Option<ConversionCandidate> {
        let candidates = match previous {
            Some(previous) => self.candidates_after(kana, previous),
            None => self.candidates(kana),
        };
        let count = kana.chars().count();
        candidates
            .into_iter()
            .find(|c| c.kana_count == count && !c.is_prediction)
    }
    /// The user committed `candidate`: remember it so it ranks higher next time.
    fn learn(&self, candidate: &ConversionCandidate, kana: &str);
    /// Pick up choices committed by another process when a text field becomes active.
    fn reload(&self) {}
    /// Persist anything learned (called on deactivation).
    fn save(&self);
}

/// The reading itself, in hiragana and katakana — the tail of `AzooKeyConverter.candidates`,
/// which every converter offers even when its dictionary gives nothing. The Windows stand-in
/// until a kanji engine exists (see the module comment).
#[derive(Default)]
pub struct KanaConverter;

impl KanaConverter {
    pub fn new() -> Self {
        Self
    }
}

impl JapaneseConverter for KanaConverter {
    fn candidates(&self, kana: &str) -> Vec<ConversionCandidate> {
        if kana.is_empty() {
            return Vec::new();
        }
        let mut out = vec![ConversionCandidate::new(kana, kana.chars().count())];
        let katakana = RomajiComposer::katakana(kana);
        if katakana != kana {
            out.push(ConversionCandidate::new(katakana, kana.chars().count()));
        }
        out
    }

    fn learn(&self, _candidate: &ConversionCandidate, _kana: &str) {}

    fn save(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_the_reading_in_both_forms() {
        let converter = KanaConverter::new();
        let candidates = converter.candidates("こんにちは");
        assert_eq!(candidates[0].text, "こんにちは");
        assert_eq!(candidates[1].text, "コンニチハ");
        assert!(candidates.iter().all(|c| c.kana_count == 5));
        assert!(converter.candidates("").is_empty());
        // Nothing to flip for a reading that is already katakana.
        assert_eq!(converter.candidates("アイウ").len(), 1);
    }
}
