//! English word + Korean particle or ending: `githubdp` → `github에`.

use crate::decide::engine::Language;
use crate::decide::mixed::MixedWord;
use crate::lang::module::ComboRule;

pub struct EnglishKorean;

impl ComboRule for EnglishKorean {
    fn language(&self) -> Language {
        Language::Mixed
    }
    fn needs(&self) -> [Language; 2] {
        [Language::English, Language::Korean]
    }
    fn compose(&self, keys: &str) -> Option<String> {
        MixedWord::split(keys).map(|m| m.text())
    }
}
