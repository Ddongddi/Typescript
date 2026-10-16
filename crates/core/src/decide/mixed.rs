//! English and Korean typed as one token: `githubdp` → `github에`, `APIfmf` → `API를`,
//! `commitgotj` → `commit해서`. The cut itself is scored by `CodeSwitch`; `MixedWord` is the
//! plain shape (an English front and the particle or ending behind it) that the learning and the
//! Esc readings still use.

use crate::decide::codeswitch::{CodeSwitch, Params};
use crate::decide::engine::{Decision, DecisionContext, DecisionEngine, Language};
use crate::decide::segmenter::JapaneseSegmenter;
use crate::lang::english::EnglishModel;
use crate::lang::hangul::{DubeolsikLayout, HangulComposer};
use crate::lang::korean::KoreanModel;
use crate::lang::particles::is_ending;
use crate::lang::romaji::RomajiComposer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedWord {
    pub english: String,
    pub korean_keys: String,
    pub korean: String,
}

impl MixedWord {
    pub fn text(&self) -> String {
        format!("{}{}", self.english, self.korean)
    }

    /// Whether these keys alone type a Korean particle or ending (rk → 가, dp → 에).
    pub fn is_ending_keys(keys: &str) -> bool {
        keys.chars()
            .next()
            .is_some_and(DubeolsikLayout::starts_syllable)
            && HangulComposer::compose(keys)
                .is_some_and(|c| c.stray_jamo().is_empty() && is_ending(&c.text()))
    }

    /// Splits off the longest Korean particle/ending at the end of `keys`. Only the shape is
    /// checked here; whether the front really is English is for the caller to know.
    pub fn split(keys: &str) -> Option<MixedWord> {
        let chars: Vec<char> = keys.chars().collect();
        // Front needs at least 2 letters; try the longest suffix first.
        if chars.len() < 3 {
            return None;
        }
        for start in 2..chars.len() {
            if !DubeolsikLayout::starts_syllable(chars[start]) {
                continue;
            }
            let suffix_keys: String = chars[start..].iter().collect();
            let Some(composed) = HangulComposer::compose(&suffix_keys) else {
                continue;
            };
            let text = composed.text();
            if composed.stray_jamo().is_empty() && is_ending(&text) {
                return Some(MixedWord {
                    english: chars[..start].iter().collect(),
                    korean_keys: suffix_keys,
                    korean: text,
                });
            }
        }
        None
    }
}

/// Decides `Mixed` for English and Korean typed as one token (with Korean on) by scoring every
/// way to cut the keys into English and Korean stretches (`CodeSwitch`), and with Japanese on
/// English + Japanese particle (githubで). Anything it doesn't cut goes to the engine behind.
pub struct SplittingEngine {
    pub base: Box<dyn DecisionEngine>,
    pub korean: KoreanModel,
    /// With Japanese on: English word + Japanese particle (githubで).
    pub japanese: Option<JapaneseSegmenter>,
    /// English + Korean splits (github에, hello월드) only happen with Korean on.
    pub korean_on: bool,
    pub codeswitch: CodeSwitch,
}

impl SplittingEngine {
    /// How sure a split is: above Strict (`AppPolicy::STRICT_THRESHOLD`), so github에 splits in
    /// a terminal too.
    pub const CONFIDENCE: f64 = 0.98;

    pub fn new(base: Box<dyn DecisionEngine>, english: &'static EnglishModel) -> Self {
        Self {
            base,
            korean: KoreanModel::default(),
            japanese: None,
            korean_on: true,
            codeswitch: CodeSwitch::new(english, Params::default()),
        }
    }

    /// With the segmenter from the heuristic: English word + Japanese particle (githubで).
    pub fn with_japanese(mut self, segmenter: JapaneseSegmenter) -> Self {
        self.japanese = Some(segmenter);
        self
    }

    /// Whether the keys spell a word people actually say in Korean (한국, guswo is 현재).
    fn is_korean_word(&self, keys: &str) -> bool {
        match HangulComposer::compose(keys) {
            Some(composed) if composed.stray_jamo().is_empty() => {
                self.korean.is_known_word(&composed.text())
            }
            _ => false,
        }
    }
}

impl DecisionEngine for SplittingEngine {
    fn decide(&self, keys: &str, context: &DecisionContext) -> Decision {
        // Clean romaji with Japanese on (hasansowo) is Japanese, not English + Hangul.
        let romaji = self.japanese.is_some() && RomajiComposer::compose(keys, false).is_clean();
        if self.korean_on && !romaji {
            if let Some(decision) = self.codeswitch.decide(keys, context) {
                return decision;
            }
        }
        // Not when the keys spell a Korean word: guswo is 현재, not gus + を (but slackwo,
        // 님차재, is no word, so slackを).
        if let Some((word, particle)) = self
            .japanese
            .as_ref()
            .and_then(|segmenter| segmenter.english_with_particle(keys))
        {
            if !self.is_korean_word(keys) {
                return Decision {
                    language: Language::Japanese,
                    confidence: Self::CONFIDENCE,
                    output: format!("{word}{particle}"),
                    explanation: format!("split {word}+{particle}"),
                    runner_up: None,
                };
            }
        }
        self.base.decide(keys, context)
    }

    fn render(&self, keys: &str, language: Language) -> String {
        // Same default as the trait, but goes through this engine's own split.
        match language {
            Language::Mixed => {
                let d = self.decide(keys, &DecisionContext::default());
                if d.language == Language::Mixed {
                    d.output
                } else {
                    MixedWord::split(keys)
                        .map(|m| m.text())
                        .unwrap_or_else(|| keys.to_string())
                }
            }
            other => self.base.render(keys, other),
        }
    }
}
