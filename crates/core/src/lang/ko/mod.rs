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
        // A single consonant key is a jamo on purpose (ㅋ of ㅋㅋ), so it stays in the Esc cycle.
        HangulComposer::compose(keys).is_some_and(|c| {
            c.stray_jamo().is_empty() || (keys.chars().count() == 1 && is_consonant(&c.text()))
        })
    }
}

fn is_consonant(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| ('ㄱ'..='ㅎ').contains(&c))
}
