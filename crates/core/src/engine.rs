//! Language decision (DecisionEngine.swift, English/Korean only).

use serde::{Deserialize, Serialize};

use crate::english::{EnglishModel, EXTRA_LEVEL};
use crate::hangul::{DubeolsikLayout, HangulComposer};
use crate::korean::KoreanModel;
use crate::mixed::MixedWord;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    English,
    Korean,
    /// English word + Korean particle/ending, e.g. `github에` (see `MixedWord`).
    Mixed,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "english",
            Language::Korean => "korean",
            Language::Mixed => "mixed",
        }
    }

    pub fn parse(s: &str) -> Option<Language> {
        match s {
            "english" => Some(Language::English),
            "korean" => Some(Language::Korean),
            "mixed" => Some(Language::Mixed),
            _ => None,
        }
    }

    /// The language a sentence continues in after this word.
    pub fn sentence_language(self) -> Language {
        match self {
            Language::English => Language::English,
            Language::Korean | Language::Mixed => Language::Korean,
        }
    }

    /// What Esc switches to: english → mixed (when the keys split) → korean → english.
    /// Readings that can't exist for these keys are skipped (no Korean for "ra-men"),
    /// and so are garbled ones as long as a clean alternative exists.
    pub fn next(self, keys: &str) -> Language {
        let order = [Language::English, Language::Mixed, Language::Korean];
        let composed = HangulComposer::compose(keys);
        let available: Vec<Language> = order
            .into_iter()
            .filter(|l| match l {
                Language::English => true,
                Language::Mixed => MixedWord::split(keys).is_some(),
                Language::Korean => composed.is_some(),
            })
            .collect();
        let clean = |l: &Language| match l {
            Language::English | Language::Mixed => true,
            Language::Korean => composed.as_ref().is_some_and(|c| c.stray_jamo().is_empty()),
        };
        let clean_count = available.iter().filter(|l| clean(l)).count();
        let cycle: Vec<Language> = if clean_count >= 2 {
            available.iter().copied().filter(|l| clean(l) || *l == self).collect()
        } else {
            available
        };
        match cycle.iter().position(|l| *l == self) {
            Some(i) => cycle[(i + 1) % cycle.len()],
            None => cycle.first().copied().unwrap_or(Language::English),
        }
    }
}

/// What the engine knows besides the token itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct DecisionContext {
    /// Language of the previously committed word, if it is recent enough to matter.
    pub previous_language: Option<Language>,
    /// The token is still being typed (used for the live preview).
    pub partial: bool,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub language: Language,
    /// 0.5 = coin flip, 1.0 = certain.
    pub confidence: f64,
    /// What should be committed for this decision.
    pub output: String,
    /// Human-readable reasoning, for the CLI and debugging.
    pub explanation: String,
    pub runner_up: Option<Language>,
}

pub trait DecisionEngine: Send + Sync {
    fn decide(&self, keys: &str, context: &DecisionContext) -> Decision;

    /// Renders `keys` in a given language regardless of what the engine would pick.
    fn render(&self, keys: &str, language: Language) -> String {
        match language {
            Language::English => keys.to_string(),
            Language::Korean => HangulComposer::compose(keys).map(|c| c.text()).unwrap_or_else(|| keys.to_string()),
            Language::Mixed => {
                // The engine's own split (hello월드, github에); the particle split for engines without one.
                let decision = self.decide(keys, &DecisionContext::default());
                if decision.language == Language::Mixed {
                    decision.output
                } else {
                    MixedWord::split(keys).map(|m| m.text()).unwrap_or_else(|| keys.to_string())
                }
            }
        }
    }
}

/// Scores the same keystrokes as English (character trigrams + word list) and Korean
/// (2-set syllables), each as average log-probability per keystroke, nudges the previous
/// word's language, and picks the best. Confidence comes from the gap to the runner-up.
pub struct HeuristicEngine {
    pub english: &'static EnglishModel,
    pub korean: KoreanModel,
    /// Added to the previous word's language score: people mostly stay in one language.
    pub context_bias: f64,
    /// Shifts Korean scores (used when the syllable model changes scale).
    pub korean_offset: f64,
    /// Taken off the dictionary floor for rarer English words (SCOWL levels 40–50: dork,
    /// ahem), so a common Korean word with the same keys (애가, 모든) wins the tie.
    pub rare_word_floor_penalty: f64,
    /// The previous word's pull is scaled by this when the keys are a known word of another
    /// language and not of the previous one: that's someone switching language.
    pub switch_bias_factor: f64,
    /// Score floors for a Korean word seen in real text (two or more syllables, one).
    pub korean_known_floor_long: f64,
    pub korean_known_floor_short: f64,
}

impl HeuristicEngine {
    pub fn new() -> Self {
        Self::with(EnglishModel::shared(), KoreanModel::default())
    }

    pub fn with(english: &'static EnglishModel, korean: KoreanModel) -> Self {
        Self {
            english,
            korean,
            context_bias: 0.8,
            korean_offset: 0.0,
            rare_word_floor_penalty: 0.3,
            switch_bias_factor: 0.25,
            korean_known_floor_long: -1.4,
            korean_known_floor_short: -2.0,
        }
    }
}

impl Default for HeuristicEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DecisionEngine for HeuristicEngine {
    fn decide(&self, keys: &str, context: &DecisionContext) -> Decision {
        // Shift on a key that has no shifted jamo (e.g. "Hello", "API") is only ever English.
        if keys.chars().any(|c| c.is_uppercase() && !DubeolsikLayout::is_korean_shift_key(c)) {
            return Decision {
                language: Language::English,
                confidence: 0.99,
                output: keys.to_string(),
                explanation: "uppercase outside QWERTOP".to_string(),
                runner_up: None,
            };
        }

        let letters: String = keys.chars().filter(|c| c.is_alphabetic()).collect();
        let count = keys.chars().count();
        let mut e = self.english.average_log_probability(&letters);
        // Also while typing: the preview must show what Space/Return will commit.
        let known = self.english.is_known_word(keys);
        if known {
            // Short dictionary words collide with real Korean (go=해, so=내, do=애),
            // so they get a weaker boost and leave room for context to decide.
            let mut floor = if count >= 4 { -1.4 } else if count == 3 { -2.0 } else { -2.6 };
            // to, so, go, do: everyday English just above a one-syllable Korean word (새, 내,
            // 해, 애), so they stay English unless the previous word was Korean.
            if count == 2 && self.english.is_common_word(keys) {
                floor = self.korean_known_floor_short + 0.1;
            }
            if self.english.level(keys).unwrap_or(0) > EXTRA_LEVEL {
                floor -= self.rare_word_floor_penalty;
            }
            e = e.max(floor);
        }
        if keys.chars().any(|c| !c.is_alphabetic()) {
            e -= 3.0; // "ra-men", "kan'i" aren't English
        }

        let mut k = self.korean.score(keys, context.partial);
        let known_korean = k
            .as_ref()
            .is_some_and(|s| s.composition.stray_jamo().is_empty() && self.korean.is_known_word_or_inflected(&s.composition.text()));
        if let (Some(score), true) = (k.as_mut(), known_korean) {
            let floor = if score.composition.syllables().len() >= 2 {
                self.korean_known_floor_long
            } else {
                self.korean_known_floor_short
            };
            score.average_log_probability = score.average_log_probability.max(floor);
        }

        let mut scores: Vec<(Language, f64)> = vec![(Language::English, e)];
        if let Some(k) = &k {
            scores.push((Language::Korean, k.average_log_probability + self.korean_offset));
        }
        if let Some(previous) = context.previous_language.map(Language::sentence_language) {
            if let Some(i) = scores.iter().position(|s| s.0 == previous) {
                let mut bias = self.context_bias;
                let known_in = |l: Language| match l {
                    Language::English => known,
                    _ => known_korean,
                };
                let other = if previous == Language::English { Language::Korean } else { Language::English };
                if !known_in(previous) && known_in(other) {
                    bias *= self.switch_bias_factor;
                }
                scores[i].1 += bias;
            }
        }
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let (language, best) = scores[0];
        let runner_up = scores.get(1).copied();
        let margin = best - runner_up.map_or(best - 5.0, |r| r.1);
        let confidence = 1.0 / (1.0 + (-margin * 2.0).exp());
        let output = match language {
            Language::Korean => k.as_ref().map(|k| k.composition.text()).unwrap_or_else(|| keys.to_string()),
            _ => keys.to_string(),
        };
        let detail: Vec<String> =
            scores.iter().map(|(l, s)| format!("{}={:.2}", &l.name()[..2], s)).collect();
        Decision {
            language,
            confidence,
            output,
            explanation: format!("{}{} margin={:+.2}", detail.join(" "), if known { " (dict)" } else { "" }, margin),
            runner_up: runner_up.map(|r| r.0),
        }
    }
}
