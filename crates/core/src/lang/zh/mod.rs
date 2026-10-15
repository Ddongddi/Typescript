//! Chinese (Mandarin, simplified Pinyin): typed latin read as pinyin syllables. The
//! dictionary word (see `lang::chinese`) is what the input method shows; the candidate
//! window comes on Space later, like Japanese kanji conversion.

use crate::decide::engine::Language;
use crate::lang::module::LanguageModule;
use crate::lang::pinyin;

pub struct Chinese;

impl LanguageModule for Chinese {
    fn language(&self) -> Language {
        Language::Chinese
    }
    fn compose(&self, keys: &str) -> Option<String> {
        pinyin::split(keys).map(|syllables| syllables.join(" "))
    }
    fn is_clean(&self, keys: &str) -> bool {
        pinyin::split(keys).is_some()
    }
}
