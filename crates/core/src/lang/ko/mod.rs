//! Korean: the keys composed as Hangul on a 2-set layout.

use crate::decide::engine::Language;
use crate::lang::hangul::HangulComposer;
use crate::lang::module::LanguageModule;

pub struct Korean;

impl LanguageModule for Korean {
    fn language(&self) -> Language {
        Language::Korean
    }
    fn compose(&self, keys: &str) -> Option<String> {
        HangulComposer::compose(keys).map(|c| c.text())
    }
    fn is_clean(&self, keys: &str) -> bool {
        HangulComposer::compose(keys).is_some_and(|c| c.stray_jamo().is_empty())
    }
}
