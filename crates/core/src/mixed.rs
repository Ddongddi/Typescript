//! English word with a Korean particle or ending typed straight after it (MixedWord.swift):
//! `githubdp` → `github에`, `APIfmf` → `API를`, `commitgotj` → `commit해서`.
//! The Japanese branches of the original are gone.

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::engine::{Decision, DecisionContext, DecisionEngine, Language};
use crate::english::EnglishModel;
use crate::hangul::{DubeolsikLayout, HangulComposer};
use crate::korean::{KoreanModel, UNKNOWN_SYLLABLE_LOGP};

/// Particles and endings that commonly attach directly to a (loan)word.
const SUFFIXES: &str = "에 에서 에게 에는 에서는 에도 에서도 의 을 를 은 는 이 가 도 로 으로 와 과 랑 이랑 하고 한테 까지 부터 만 보다 \
처럼 이나 나 이야 야 이에요 예요 이다 입니다 인데 이라 이라고 라고 으로는 로는 으로도 로도 이고 이지 이랑은 \
했어 했다 했어요 했는데 했고 했지 했음 해서 해야 하는 하면 하고 하기 하게 하다 해 해요 합니다 할 한 함 하자 해줘 해봐 \
됐어 됐다 된 되는 되면 돼 돼요 됩니다 같은 같아 적 적인 적으로 스럽게 같이 들 들이 들을 들은 에요 이요 를요 을요 \
같은건 같은거 같은게 같은걸 같은데 같다 같네 같아서 같은지 같고 같아요 인가 인지 인데요 인건 인게 인거 \
이라는 라는 이란 에선 으론 로서 으로서";

/// Particles that follow a single letter (x에, n이, a를, X축의 X): case particles only, not
/// endings like 해/한, so short English (ago) doesn't split.
const SINGLE_LETTER_PARTICLES: &str = "에 에서 에게 에는 에서는 에도 의 을 를 은 는 이 가 도 로 으로 와 과 랑 이랑 하고 한테 까지 부터 만 보다 \
처럼 이나 나 이라 이라고 라고 이랑은 으로는 로는 같은 같이";

static SUFFIX_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| SUFFIXES.split_whitespace().collect());
static SINGLE_SET: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| SINGLE_LETTER_PARTICLES.split_whitespace().collect());

pub fn is_suffix(s: &str) -> bool {
    SUFFIX_SET.contains(s)
}

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

    /// A single letter and a particle: `xdp` → `x에`, `Xfmf` → `X를`. One key on its own can't
    /// make a Hangul syllable before another consonant, so the keys never read as Korean.
    pub fn split_single_letter(keys: &str) -> Option<MixedWord> {
        let mut chars = keys.chars();
        let first = chars.next()?;
        if keys.chars().count() < 2 || !first.is_ascii() || !first.is_alphabetic() {
            return None;
        }
        let suffix_keys: String = chars.collect();
        if !DubeolsikLayout::starts_syllable(suffix_keys.chars().next()?) {
            return None;
        }
        let composed = HangulComposer::compose(&suffix_keys)?;
        let text = composed.text();
        if !composed.stray_jamo().is_empty() || !SINGLE_SET.contains(text.as_str()) {
            return None;
        }
        // 도 and 나 are the keys eh and sk, common English endings (heh, meh, yeh, tsk): after a
        // lowercase letter they stay English. A capital (X도) was typed on purpose.
        if first.is_lowercase() && (text == "도" || text == "나") {
            return None;
        }
        Some(MixedWord { english: first.to_string(), korean_keys: suffix_keys, korean: text })
    }

    /// Splits off the longest Korean particle/ending at the end of `keys`. Only the shape is
    /// checked here; whether the front really is English is up to the caller (`SplittingEngine`).
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
            let Some(composed) = HangulComposer::compose(&suffix_keys) else { continue };
            let text = composed.text();
            if composed.stray_jamo().is_empty() && SUFFIX_SET.contains(text.as_str()) {
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

/// Decides `Mixed` for English and Korean typed as one token: English + particle (github에),
/// a single letter + particle (x에), English + Korean word (hello월드). Never when the whole
/// token is a known English word or reads as ordinary Korean.
pub struct SplittingEngine {
    pub base: Box<dyn DecisionEngine>,
    pub english: &'static EnglishModel,
    pub korean: KoreanModel,
    /// The front counts as English if it's in the word list, or spells like English
    /// (trigram score at least this) and can't be read as Hangul.
    pub english_spelling_floor: f64,
    /// English word + Korean word (hello월드) only when the whole token spells less like
    /// English than this; stricter still when the Korean half isn't in the word lists.
    pub mixed_word_spelling_ceiling: f64,
    pub unknown_word_spelling_ceiling: f64,
}

impl SplittingEngine {
    /// How sure a split is: above Strict (`AppPolicy::STRICT_THRESHOLD`), since each rule already
    /// asks for an English front it can stand on, so github에 splits in a terminal too.
    pub const CONFIDENCE: f64 = 0.98;

    pub fn new(base: Box<dyn DecisionEngine>, english: &'static EnglishModel) -> Self {
        Self {
            base,
            english,
            korean: KoreanModel::default(),
            english_spelling_floor: -2.6,
            mixed_word_spelling_ceiling: -2.7,
            unknown_word_spelling_ceiling: -3.2,
        }
    }

    fn split_decision(mixed: &MixedWord) -> Decision {
        Decision {
            language: Language::Mixed,
            confidence: Self::CONFIDENCE,
            output: mixed.text(),
            explanation: format!("split {}+{}", mixed.english, mixed.korean),
            runner_up: None,
        }
    }

    /// UI, AI, OS, API: capitals typed on purpose, with at least one that has no shifted
    /// jamo (so it can't be ㅃㅉㄸㄲㅆㅒㅖ). Short ones count too (UI같은건).
    fn is_acronym(front: &str) -> bool {
        (2..=6).contains(&front.chars().count())
            && front.chars().all(|c| c.is_uppercase())
            && front.chars().any(|c| !DubeolsikLayout::is_korean_shift_key(c))
    }

    /// Slack, TextEdit, VSCode, GitHub: a name typed with its capitals, so English even when
    /// it isn't in the word list.
    fn is_capitalized_name(front: &str) -> bool {
        let chars: Vec<char> = front.chars().collect();
        if chars.len() < 3 || !chars[0].is_uppercase() || !chars.iter().any(|c| c.is_lowercase()) {
            return false;
        }
        if Self::has_english_only_capital(front) {
            return true;
        }
        if !chars[1..].iter().any(|c| c.is_uppercase()) {
            return false;
        }
        // Every capital a shifted jamo and the keys spelling whole Hangul is Korean typed with
        // shift (RoRmtgkgfu, 깨끗핳려), not a name (TextEdit).
        HangulComposer::compose(front).map(|c| !c.stray_jamo().is_empty()).unwrap_or(true)
    }

    fn has_english_only_capital(front: &str) -> bool {
        front.chars().any(|c| c.is_uppercase() && !DubeolsikLayout::is_korean_shift_key(c))
    }

    fn looks_english(&self, front: &str) -> bool {
        if self.english.is_known_word(front) {
            return true;
        }
        let unreadable = HangulComposer::compose(front).map(|c| !c.stray_jamo().is_empty()).unwrap_or(true);
        unreadable && self.english.average_log_probability(front) >= self.english_spelling_floor
    }

    fn reads_as_korean(&self, keys: &str) -> bool {
        match HangulComposer::compose(keys) {
            Some(c) if c.stray_jamo().is_empty() => {
                c.syllables().iter().all(|s| self.korean.syllable_log_probability(*s) > UNKNOWN_SYLLABLE_LOGP)
            }
            _ => false,
        }
    }

    fn is_korean_word_or_word_with_particle(&self, hangul: &str) -> bool {
        if self.korean.is_vocabulary_word(hangul) {
            return true;
        }
        let chars: Vec<char> = hangul.chars().collect();
        (2..chars.len()).any(|cut| {
            let (head, tail): (String, String) = (chars[..cut].iter().collect(), chars[cut..].iter().collect());
            self.korean.is_vocabulary_word(&head) && SUFFIX_SET.contains(tail.as_str())
        })
    }

    /// An English word with a whole Korean word typed straight after it, no space:
    /// `hellodnjfem` → `hello월드`. The front has to be a known English word (or a name typed
    /// with capitals) and the back at least two syllables; prefers the longest English front.
    fn english_then_korean_word(&self, keys: &str) -> Option<MixedWord> {
        let chars: Vec<char> = keys.chars().collect();
        if chars.len() < 5 || self.english.is_known_word(keys) {
            return None;
        }
        let spelling = self.english.average_log_probability(keys);
        // ponytail: the original also rejects clean romaji (hasansowo) here; Japanese is out of scope.
        if spelling >= self.mixed_word_spelling_ceiling {
            return None;
        }
        for start in (2..=chars.len() - 2).rev() {
            let front: String = chars[..start].iter().collect();
            let back: String = chars[start..].iter().collect();
            // Capitals count only when one has no Korean meaning (ThkT is 쐈, not a name).
            let front_ok = (Self::has_english_only_capital(&front)
                && (Self::is_acronym(&front) || Self::is_capitalized_name(&front)))
                || (start >= 3 && self.english.is_known_word(&front));
            if !front_ok || !DubeolsikLayout::starts_syllable(back.chars().next()?) {
                continue;
            }
            let Some(composed) = HangulComposer::compose(&back) else { continue };
            if !composed.stray_jamo().is_empty() || composed.syllables().len() < 2 {
                continue;
            }
            let text = composed.text();
            let known = self.is_korean_word_or_word_with_particle(&text)
                || (spelling < self.unknown_word_spelling_ceiling
                    && composed
                        .syllables()
                        .iter()
                        .all(|s| self.korean.syllable_log_probability(*s) > UNKNOWN_SYLLABLE_LOGP));
            if !known {
                continue;
            }
            // A name typed with a capital is English whatever the keys also spell; otherwise
            // keys that read as ordinary Korean on their own stay Korean.
            if !Self::has_english_only_capital(&front) && self.reads_as_korean(keys) {
                return None;
            }
            return Some(MixedWord { english: front, korean_keys: back, korean: text });
        }
        None
    }
}

impl DecisionEngine for SplittingEngine {
    fn decide(&self, keys: &str, context: &DecisionContext) -> Decision {
        if let Some(mixed) = MixedWord::split(keys) {
            let front_ok = Self::is_acronym(&mixed.english)
                || Self::is_capitalized_name(&mixed.english)
                || (mixed.english.chars().count() >= 3 && self.looks_english(&mixed.english));
            // A capital with no shifted jamo (the S of Slack) was typed as English on purpose,
            // even if the letters would also spell Korean (slackdptj, 님차에서).
            if front_ok
                && !self.english.is_known_word(keys)
                && (Self::has_english_only_capital(&mixed.english) || !self.reads_as_korean(keys))
            {
                return Self::split_decision(&mixed);
            }
        }
        if let Some(mixed) = MixedWord::split_single_letter(keys) {
            if !self.english.is_known_word(keys) && !self.reads_as_korean(keys) {
                return Self::split_decision(&mixed);
            }
        }
        if let Some(mixed) = self.english_then_korean_word(keys) {
            return Self::split_decision(&mixed);
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
                    MixedWord::split(keys).map(|m| m.text()).unwrap_or_else(|| keys.to_string())
                }
            }
            other => self.base.render(keys, other),
        }
    }
}
