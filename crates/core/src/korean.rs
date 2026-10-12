//! Syllable-unigram model for Korean (KoreanModel.swift): tells "안녕하세요" from the
//! accidental Hangul English produces on a 2-set layout (e.g. "the" → "솓").

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::hangul::{is_consonant, Composed, HangulComposer, Unit};
use crate::sorted::SortedLines;
use crate::syllables::{TIER1, TIER2, TIER3};

const KOREAN_SYLLABLES: &str = include_str!("../data/korean_syllables.txt");
// Sorted and deduplicated by build.rs, searched in place.
const KOREAN_WORDS: &str = include_str!(concat!(env!("OUT_DIR"), "/korean_words_sorted.txt"));
const KOREAN_VOCABULARY: &str = include_str!(concat!(env!("OUT_DIR"), "/korean_vocab_sorted.txt"));

pub const TIER1_LOGP: f64 = -5.0;
pub const TIER2_LOGP: f64 = -6.5;
pub const TIER3_LOGP: f64 = -8.0;
pub const UNKNOWN_SYLLABLE_LOGP: f64 = -11.0;
pub const STRAY_JAMO_LOGP: f64 = -13.0;
/// Typing the first consonant of the next syllable is normal mid-word.
pub const PENDING_CONSONANT_LOGP: f64 = -3.5;
/// ㅋㅋ, ㅎㅎ, ㅠㅠ and friends.
pub const CHAT_JAMO_LOGP: f64 = -4.0;
const CHAT_JAMO: &str = "ㅋㅎㅠㅜㄷㅇㄱㅅㅂㄴ";

/// Words seen in Korean sentences (korean_words.txt).
static WORDS: LazyLock<SortedLines> = LazyLock::new(|| SortedLines::new(KOREAN_WORDS));

static VOCABULARY: LazyLock<SortedLines> = LazyLock::new(|| SortedLines::new(KOREAN_VOCABULARY));

/// Where syllable probabilities come from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Tiers,
    Corpus,
    /// Whichever of the two rates a syllable higher.
    Max,
}

pub struct KoreanScore {
    /// Average log-probability per keystroke.
    pub average_log_probability: f64,
    pub composition: Composed,
}

pub struct KoreanModel {
    tiers: HashMap<char, f64>,
}

fn is_hangul_syllable(c: char) -> bool {
    ('가'..='힣').contains(&c)
}

impl Default for KoreanModel {
    fn default() -> Self {
        Self::new(Source::Max)
    }
}

impl KoreanModel {
    pub fn new(source: Source) -> Self {
        let mut map: HashMap<char, f64> = HashMap::new();
        if source != Source::Corpus {
            // later (more frequent) tiers overwrite earlier ones
            for (syllables, logp) in [(TIER3, TIER3_LOGP), (TIER2, TIER2_LOGP), (TIER1, TIER1_LOGP)] {
                for s in syllables.chars().filter(|c| is_hangul_syllable(*c)) {
                    map.insert(s, logp);
                }
            }
        }
        if source != Source::Tiers {
            let counts: Vec<(char, f64)> = KOREAN_SYLLABLES
                .lines()
                .filter_map(|l| {
                    let mut it = l.split('\t');
                    let s = it.next()?.chars().next()?;
                    let n: f64 = it.next()?.trim().parse().ok()?;
                    Some((s, n))
                })
                .collect();
            let total: f64 = counts.iter().map(|c| c.1).sum();
            let (smoothing, possible) = (0.5, 11172.0);
            for (s, n) in counts {
                let logp = ((n + smoothing) / (total + smoothing * possible)).ln();
                let best = map.get(&s).copied().unwrap_or(f64::NEG_INFINITY).max(logp);
                map.insert(s, best);
            }
        }
        Self { tiers: map }
    }

    /// A word that occurs in real Korean text (the composed Hangul, not the keys).
    pub fn is_known_word(&self, hangul: &str) -> bool {
        WORDS.contains(hangul)
    }

    /// A word in the wider vocabulary (loanwords like 월드, 서버), for English + Korean splits.
    pub fn is_vocabulary_word(&self, hangul: &str) -> bool {
        WORDS.contains(hangul) || VOCABULARY.contains(hangul)
    }

    pub fn syllable_log_probability(&self, syllable: char) -> f64 {
        self.tiers.get(&syllable).copied().unwrap_or(UNKNOWN_SYLLABLE_LOGP)
    }

    /// `partial`: the token is still being typed, so a trailing lone consonant is
    /// expected rather than penalized.
    pub fn score(&self, keys: &str, partial: bool) -> Option<KoreanScore> {
        if keys.is_empty() {
            return None;
        }
        let composition = HangulComposer::compose(keys)?;
        let units = &composition.units;
        // ㅋㅋ ㅎㅎ ㅠㅠ ㄷㄷ…; and any two consonants (ㄹㅇ ㅇㅈ ㅈㅅ ㄱㅊ ㅁㅊ), which the original
        // misses. Two only: longer all-consonant keys spell English words (tea, set, cat).
        let all_chat = units.len() >= 2
            && (units.iter().all(|u| matches!(u, Unit::Jamo(j) if CHAT_JAMO.contains(*j)))
                || (units.len() == 2 && units.iter().all(|u| matches!(u, Unit::Jamo(j) if is_consonant(*j)))));
        let mut total = 0.0;
        for (i, unit) in units.iter().enumerate() {
            total += match unit {
                Unit::Syllable(s) => self.syllable_log_probability(*s),
                Unit::Jamo(j) => {
                    if all_chat {
                        CHAT_JAMO_LOGP
                    } else if partial && i == units.len() - 1 && is_consonant(*j) {
                        PENDING_CONSONANT_LOGP
                    } else {
                        STRAY_JAMO_LOGP
                    }
                }
            };
        }
        Some(KoreanScore { average_log_probability: total / keys.chars().count() as f64, composition })
    }
}
