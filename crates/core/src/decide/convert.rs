//! Kana → kanji conversion (JapaneseConverter.h in the original: `AzooKeyConverter` in the
//! app, `AppleJapaneseConverter` where macOS's engine can be loaded, fakes in tests).
//!
//! Both engines of the original are macOS-only: `AppleJapaneseConverter` loads the private
//! libmecabra at runtime, and `AzooKeyConverter` links AzooKey's Swift package with its own
//! bundled dictionary. Neither can be ported. This module keeps the same interface and ships
//! `KanaConverter`, which offers the forms every converter guarantees (the reading as hiragana
//! and as katakana): Japanese typing, Esc reading cycling, Delete, the candidate flow and live
//! conversion all work; kanji conversion waits for a Windows engine behind the same trait.

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
}

impl ConversionCandidate {
    pub fn new(text: impl Into<String>, kana_count: usize) -> Self {
        Self {
            text: text.into(),
            kana_count,
            is_prediction: false,
        }
    }

    pub fn prediction(text: impl Into<String>, kana_count: usize) -> Self {
        Self {
            text: text.into(),
            kana_count,
            is_prediction: true,
        }
    }
}

/// Kana → kanji conversion.
pub trait JapaneseConverter: Send + Sync {
    fn candidates(&self, kana: &str) -> Vec<ConversionCandidate>;
    /// The user committed `candidate`: remember it so it ranks higher next time.
    fn learn(&self, candidate: &ConversionCandidate, kana: &str);
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
