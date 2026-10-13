//! Language decision (DecisionEngine.swift, with the Japanese model this port adds).

use serde::{Deserialize, Serialize};

use crate::decide::segmenter::{JapaneseSegmenter, Segment};
use crate::lang::english::{EnglishModel, EXTRA_LEVEL};
use crate::lang::hangul::DubeolsikLayout;
use crate::lang::japanese::JapaneseModel;
use crate::lang::korean::{KoreanModel, KoreanScore};
use crate::lang::registry;
use crate::lang::set::Languages;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    English,
    Korean,
    /// English word + Korean particle/ending, e.g. `github에` (see `MixedWord`).
    Mixed,
    /// Romaji typed as Japanese (rendered as hiragana; kanji conversion happens on Space).
    Japanese,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "english",
            Language::Korean => "korean",
            Language::Mixed => "mixed",
            Language::Japanese => "japanese",
        }
    }

    /// The short id used in settings.json and pack names.
    pub fn id(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Korean => "ko",
            Language::Mixed => "mixed",
            Language::Japanese => "ja",
        }
    }

    /// A language the user can turn on (not `Mixed`, which comes from English + Korean).
    pub fn from_id(id: &str) -> Option<Language> {
        match id {
            "en" => Some(Language::English),
            "ko" => Some(Language::Korean),
            "ja" => Some(Language::Japanese),
            _ => None,
        }
    }

    pub fn parse(s: &str) -> Option<Language> {
        match s {
            "english" => Some(Language::English),
            "korean" => Some(Language::Korean),
            "mixed" => Some(Language::Mixed),
            "japanese" => Some(Language::Japanese),
            _ => None,
        }
    }

    /// The language a sentence continues in after this word.
    pub fn sentence_language(self) -> Language {
        match self {
            Language::English => Language::English,
            Language::Korean | Language::Mixed => Language::Korean,
            Language::Japanese => Language::Japanese,
        }
    }

    /// What Esc switches to: the enabled languages in the user's order, the English + Korean
    /// reading right after English. Readings that can't exist for these keys are skipped
    /// (no Korean for "ra-men"), and so are garbled ones (dkssud as Japanese: dkっすd) as long
    /// as a clean alternative exists — a typo can still be forced into its language when
    /// nothing reads cleanly.
    pub fn next(self, keys: &str, languages: &Languages) -> Language {
        let available: Vec<Language> = registry::cycle_order(languages)
            .into_iter()
            .filter(|l| registry::reading(*l, keys).is_some())
            .collect();
        let clean = |l: &Language| registry::is_clean(*l, keys);
        let clean_count = available.iter().filter(|l| clean(l)).count();
        let cycle: Vec<Language> = if clean_count >= 2 {
            available
                .iter()
                .copied()
                .filter(|l| clean(l) || *l == self)
                .collect()
        } else {
            available
        };
        match cycle.iter().position(|l| *l == self) {
            Some(i) => cycle[(i + 1) % cycle.len()],
            None => cycle.first().copied().unwrap_or(languages.first()),
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
        if language == Language::Mixed {
            // The engine's own split (hello월드, github에); the particle split for engines without one.
            let decision = self.decide(keys, &DecisionContext::default());
            if decision.language == Language::Mixed {
                return decision.output;
            }
        }
        registry::reading(language, keys).unwrap_or_else(|| keys.to_string())
    }
}

/// Score floors for a dictionary English word by length: short ones collide with real Korean.
const ENGLISH_FLOOR_LONG: f64 = -1.4;
const ENGLISH_FLOOR_MEDIUM: f64 = -2.0;
const ENGLISH_FLOOR_SHORT: f64 = -2.6;
/// Taken off the English score when the token has non-letters.
const NON_LETTER_PENALTY: f64 = 3.0;
/// Confidence for keys with a capital outside QWERTOP: only ever English.
const CAPITALISED_ENGLISH_CONFIDENCE: f64 = 0.99;

/// Scores the same keystrokes as English (character trigrams + word list), Korean
/// (2-set syllables) and, when enabled, Japanese (romaji → kana trigrams), each as
/// average log-probability per keystroke, nudges the previous word's language, and
/// picks the best. Confidence comes from the gap to the runner-up.
pub struct HeuristicEngine {
    pub english: &'static EnglishModel,
    pub korean: KoreanModel,
    /// None when Japanese is off or its model isn't available.
    pub japanese: Option<&'static JapaneseModel>,
    /// Added to the previous word's language score: people mostly stay in one language.
    pub context_bias: f64,
    /// Bias after Japanese text. Raising it to 1.2 recovered 1 of 2,590 held-out phrases
    /// but tripled common English words turning into kana right after Japanese (0.5% → 1.6%).
    pub japanese_context_bias: f64,
    /// Shifts Japanese scores to the English/Korean scale (kana trigrams over ~80 symbols
    /// per ~2 keys read differently from letter trigrams). Tuned on held-out data so that
    /// English words almost never turn into kana.
    pub japanese_offset: f64,
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
    /// How far Japanese must lead before a known English word is turned into kana
    /// outside Japanese text.
    pub english_word_protection: f64,
    /// With Japanese on: English words inside a phrase (`googlede…`) score as segmented
    /// Japanese, not as kana with stray letters.
    pub segmenter: Option<JapaneseSegmenter>,
}

impl HeuristicEngine {
    pub fn new() -> Self {
        Self::with(EnglishModel::shared(), KoreanModel::default())
    }

    pub fn with(english: &'static EnglishModel, korean: KoreanModel) -> Self {
        Self {
            english,
            korean,
            japanese: None,
            context_bias: 0.8,
            japanese_context_bias: 0.8,
            japanese_offset: 0.5,
            korean_offset: 0.0,
            rare_word_floor_penalty: 0.3,
            switch_bias_factor: 0.25,
            korean_known_floor_long: -1.4,
            korean_known_floor_short: -2.0,
            english_word_protection: 1.0,
            segmenter: None,
        }
    }

    /// The heuristic with Japanese scoring and segmentation on (the app's `makeEngine`).
    pub fn with_japanese(english: &'static EnglishModel, japanese: &'static JapaneseModel) -> Self {
        let mut engine = Self::with(english, KoreanModel::default());
        engine.japanese = Some(japanese);
        engine.segmenter = Some(JapaneseSegmenter::new(english, japanese));
        engine
    }
}

impl HeuristicEngine {
    /// Average log-probability of `keys` read as English: character trigrams, lifted to a floor
    /// for dictionary words.
    fn english_score(&self, keys: &str, known: bool) -> f64 {
        let letters: String = keys.chars().filter(|c| c.is_alphabetic()).collect();
        let count = keys.chars().count();
        let mut e = self.english.average_log_probability(&letters);
        // Also while typing: the preview must show what Space/Return will commit.
        if known {
            // Short dictionary words collide with real Korean (go=해, so=내, do=애),
            // so they get a weaker boost and leave room for context to decide.
            let mut floor = match count {
                4.. => ENGLISH_FLOOR_LONG,
                3 => ENGLISH_FLOOR_MEDIUM,
                _ => ENGLISH_FLOOR_SHORT,
            };
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
            e -= NON_LETTER_PENALTY; // "ra-men", "kan'i" aren't English
        }
        e
    }

    /// The Korean reading of `keys` with its score, and whether it is a word seen in real
    /// text (which lifts the score to a floor).
    fn korean_score(&self, keys: &str, partial: bool) -> (Option<KoreanScore>, bool) {
        let mut k = self.korean.score(keys, partial);
        let known = k.as_ref().is_some_and(|s| {
            s.composition.stray_jamo().is_empty()
                && self
                    .korean
                    .is_known_word_or_inflected(&s.composition.text())
        });
        if let (Some(score), true) = (k.as_mut(), known) {
            let floor = if score.composition.syllables().len() >= 2 {
                self.korean_known_floor_long
            } else {
                self.korean_known_floor_short
            };
            score.average_log_probability = score.average_log_probability.max(floor);
        }
        (k, known)
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
        if keys
            .chars()
            .any(|c| c.is_uppercase() && !DubeolsikLayout::is_korean_shift_key(c))
        {
            return Decision {
                language: Language::English,
                confidence: CAPITALISED_ENGLISH_CONFIDENCE,
                output: keys.to_string(),
                explanation: "uppercase outside QWERTOP".to_string(),
                runner_up: None,
            };
        }

        let known = self.english.is_known_word(keys);
        let e = self.english_score(keys, known);

        let (k, known_korean) = self.korean_score(keys, context.partial);

        // A dictionary English word may already be complete: score its Japanese reading as
        // finished too, so the preview shows what Space will commit ("to" stays to, not と).
        let j = self
            .japanese
            .and_then(|model| model.score(keys, context.partial && !known));
        // English words inside a Japanese phrase (googlede…) score as segmented Japanese,
        // not as kana with stray letters.
        let mut segmented: Option<(Vec<Segment>, f64)> = None;
        if let Some(score) = &j {
            if score.composition.stray_letters > 0 {
                segmented = self
                    .segmenter
                    .as_ref()
                    .and_then(|s| s.scored_segments(keys));
            }
        }
        let count = keys.chars().count() as f64;
        let japanese_score = j.as_ref().map(|score| {
            score.average_log_probability.max(
                segmented
                    .as_ref()
                    .map_or(f64::NEG_INFINITY, |(_, s)| s / count),
            )
        });

        let mut scores: Vec<(Language, f64)> = vec![(Language::English, e)];
        if let Some(k) = &k {
            scores.push((
                Language::Korean,
                k.average_log_probability + self.korean_offset,
            ));
        }
        if let Some(japanese_score) = japanese_score {
            scores.push((Language::Japanese, japanese_score + self.japanese_offset));
        }
        if let Some(previous) = context.previous_language.map(Language::sentence_language) {
            if let Some(i) = scores.iter().position(|s| s.0 == previous) {
                let mut bias = if previous == Language::Japanese {
                    self.japanese_context_bias
                } else {
                    self.context_bias
                };
                let known_in = |l: Language| match l {
                    Language::English => known,
                    Language::Korean => known_korean,
                    _ => false,
                };
                if !known_in(previous)
                    && [Language::English, Language::Korean]
                        .iter()
                        .any(|l| *l != previous && known_in(*l))
                {
                    bias *= self.switch_bias_factor;
                }
                scores[i].1 += bias;
            }
        }
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // A dictionary English word only becomes kana on a clear win, unless we're already
        // in Japanese text (where "same", "sake", "kite" are Japanese words).
        if known
            && scores[0].0 == Language::Japanese
            && context.previous_language.map(Language::sentence_language)
                != Some(Language::Japanese)
        {
            if let Some(i) = scores.iter().position(|s| s.0 == Language::English) {
                if scores[0].1 - scores[i].1 < self.english_word_protection {
                    scores.swap(0, i);
                }
            }
        }

        let (language, best) = scores[0];
        let runner_up = scores.get(1).copied();
        let margin = best - runner_up.map_or(best - 5.0, |r| r.1);
        let confidence = 1.0 / (1.0 + (-margin * 2.0).exp());
        let output = match language {
            Language::Korean => k
                .as_ref()
                .map(|k| k.composition.text())
                .unwrap_or_else(|| keys.to_string()),
            Language::Japanese => segmented
                .map(|(segments, _)| JapaneseSegmenter::text(&segments))
                .or_else(|| j.as_ref().map(|score| score.composition.text.clone()))
                .unwrap_or_else(|| keys.to_string()),
            _ => keys.to_string(),
        };
        let detail: Vec<String> = scores
            .iter()
            .map(|(l, s)| format!("{}={:.2}", &l.name()[..2], s))
            .collect();
        Decision {
            language,
            confidence,
            output,
            explanation: format!(
                "{}{} margin={:+.2}",
                detail.join(" "),
                if known { " (dict)" } else { "" },
                margin
            ),
            runner_up: runner_up.map(|r| r.0),
        }
    }
}
