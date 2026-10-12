//! Syllable-unigram model for Korean (KoreanModel.swift): tells "안녕하세요" from the
//! accidental Hangul English produces on a 2-set layout (e.g. "the" → "솓").

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::hangul::{is_consonant, Composed, HangulComposer, Unit};
use crate::mixed::is_suffix;
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

/// The initial consonants in syllable order, as the lone jamo a half-typed syllable shows.
const INITIALS: &str = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ";

/// (initial, vowel, final) indices of a precomposed syllable.
fn parts(c: char) -> Option<(usize, usize, usize)> {
    let i = (c as usize).checked_sub('가' as usize).filter(|i| *i < 11172)?;
    Some((i / 588, i % 588 / 28, i % 28))
}

/// Whether `full` can be what the half-typed `partial` is becoming: the same syllable, a
/// syllable with the final consonant still to come (하 → 한), or a lone consonant (ㅎ → 하).
fn fits(partial: char, full: char) -> bool {
    if partial == full {
        return true;
    }
    let Some(f) = parts(full) else { return false };
    match parts(partial) {
        Some(p) => p.0 == f.0 && p.1 == f.1 && (p.2 == 0 || p.2 == f.2),
        None => INITIALS.chars().position(|c| c == partial) == Some(f.0),
    }
}

/// Whether `word` continues `prefix`, whose last character may be half-typed Hangul.
pub fn continues(prefix: &str, word: &str) -> bool {
    let Some(last) = prefix.chars().next_back() else { return true };
    let head = &prefix[..prefix.len() - last.len_utf8()];
    word.strip_prefix(head).and_then(|rest| rest.chars().next()).is_some_and(|c| fits(last, c))
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

    /// A known word, or a vocabulary word (two syllables or more) with a particle/ending
    /// straight after it (서버를, 월드에서).
    pub fn is_known_word_or_inflected(&self, hangul: &str) -> bool {
        if self.is_known_word(hangul) {
            return true;
        }
        hangul.char_indices().skip(2).any(|(cut, _)| {
            let (head, tail) = hangul.split_at(cut);
            is_suffix(tail) && self.is_vocabulary_word(head)
        })
    }

    /// A word in the wider vocabulary (loanwords like 월드, 서버), for English + Korean splits.
    pub fn is_vocabulary_word(&self, hangul: &str) -> bool {
        WORDS.contains(hangul) || VOCABULARY.contains(hangul)
    }

    /// The most commonly said word that continues `prefix` (autocomplete), where the syllable
    /// being typed may still be unfinished (안녀 → 안녕하세요); None under two syllables.
    pub fn complete(&self, prefix: &str) -> Option<String> {
        if prefix.chars().count() < 2 {
            return None;
        }
        let head = &prefix[..prefix.len() - prefix.chars().next_back()?.len_utf8()];
        let column = |line: &str| line.split('\t').nth(1).and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
        let word = |line: &'static str| line.split('\t').next().unwrap_or(line);
        let candidates = |list: &'static SortedLines| {
            list.with_prefix(head).filter(move |l| word(l) != prefix && continues(prefix, word(l)))
        };
        // Spoken-language ranking first (1 = most said), then the Tatoeba counts.
        if let Some(best) = candidates(&VOCABULARY).min_by_key(|l| column(l)) {
            return Some(word(best).to_string());
        }
        candidates(&WORDS).max_by_key(|l| column(l)).map(|l| word(l).to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_word_with_particle_counts_as_known() {
        let m = KoreanModel::default();
        assert!(m.is_known_word_or_inflected("서버를"));
        assert!(!m.is_known_word_or_inflected("서버를를를"));
        assert!(!m.is_known_word_or_inflected("솓솓"));
    }
}
