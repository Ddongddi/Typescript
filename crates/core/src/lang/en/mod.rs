//! English: the keys as typed.

use crate::decide::engine::Language;
use crate::lang::module::LanguageModule;

pub struct English;

impl LanguageModule for English {
    fn language(&self) -> Language {
        Language::English
    }
    fn compose(&self, keys: &str) -> Option<String> {
        Some(keys.to_string())
    }
    fn is_clean(&self, _keys: &str) -> bool {
        true
    }
}
