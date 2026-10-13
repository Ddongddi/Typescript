//! Japanese: romaji read as kana (kanji conversion happens on Space, in the session).

use crate::decide::engine::Language;
use crate::lang::module::LanguageModule;
use crate::lang::romaji::RomajiComposer;

pub struct Japanese;

impl LanguageModule for Japanese {
    fn language(&self) -> Language {
        Language::Japanese
    }
    fn compose(&self, keys: &str) -> Option<String> {
        Some(RomajiComposer::compose(keys, false).text)
    }
    fn is_clean(&self, keys: &str) -> bool {
        RomajiComposer::compose(keys, false).is_clean()
    }
}
