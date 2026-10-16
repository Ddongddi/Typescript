//! Syllable-unigram model for Korean (KoreanModel.swift): tells "안녕하세요" from the
//! accidental Hangul English produces on a 2-set layout (e.g. "the" → "솓").

use std::borrow::Cow;
use std::collections::HashMap;

use crate::decide::engine::Language;
use crate::lang::hangul::{is_consonant, Composed, HangulComposer, Unit};
use crate::lang::pack::{self, PackLazy};
use crate::lang::particles::is_ending;
use crate::lang::sorted::SortedLines;
use crate::lang::syllables::{TIER1, TIER2, TIER3};

pub const TIER1_LOGP: f64 = -5.0;
pub const TIER2_LOGP: f64 = -6.5;
pub const TIER3_LOGP: f64 = -8.0;
pub const UNKNOWN_SYLLABLE_LOGP: f64 = -11.0;
pub const STRAY_JAMO_LOGP: f64 = -13.0;
/// Keys with exactly one stray jamo score at most this once they are this long (the penalty is
/// otherwise averaged over the keystrokes and fades).
const STRAY_WORD_LOGP: f64 = -9.0;
const STRAY_WORD_MIN_KEYS: usize = 4;
/// Longer tokens are English + Korean typed together (dhsmfdmsgoogledptj): leave those alone.
const STRAY_WORD_MAX_KEYS: usize = 9;
/// Typing the first consonant of the next syllable is normal mid-word.
pub const PENDING_CONSONANT_LOGP: f64 = -3.5;
/// ㅋㅋ, ㅎㅎ, ㅠㅠ and friends.
pub const CHAT_JAMO_LOGP: f64 = -4.0;
const CHAT_JAMO: &str = "ㅋㅎㅠㅜㄷㅇㄱㅅㅂㄴ";

/// Sum of the counts in korean_words.txt.
const WORDS_TOTAL: f64 = 69_953.0;
/// A vocabulary word of rank r costs RANK_LOGP + RANK_SLOPE·ln r (rank 1 … 48735).
const RANK_LOGP: f64 = 5.0;
const RANK_SLOPE: f64 = 0.65;

/// How often each initial consonant, vowel and final consonant occurs in the syllables of real
/// text: prices a syllable no text has shown (켜) by its parts instead of calling it nonsense.
struct JamoPrior {
    initial: [f64; 19],
    vowel: [f64; 21],
    last: [f64; 28],
}

/// Log probability of each syllable seen in korean_syllable_counts.txt (a large sample of text;
/// the smaller korean_syllables.txt behind `tiers` is what the scores of whole words were fitted to).
static SYLLABLE_LOGP: PackLazy<HashMap<char, f64>> = PackLazy::new(Language::Korean, || {
    let counts: Vec<(char, f64)> = pack::text(Language::Korean, "korean_syllable_counts.txt")
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let s = fields.next()?.chars().next()?;
            Some((s, fields.next()?.trim().parse().ok()?))
        })
        .collect();
    let total: f64 = counts.iter().map(|c| c.1).sum();
    let smoothing = 0.5;
    counts
        .into_iter()
        .map(|(s, n)| (s, ((n + smoothing) / (total + smoothing * 11172.0)).ln()))
        .collect()
});

static JAMO_PRIOR: PackLazy<JamoPrior> = PackLazy::new(Language::Korean, || {
    let mut counts = ([1.0; 19], [1.0; 21], [1.0; 28]);
    for line in pack::text(Language::Korean, "korean_syllable_counts.txt").lines() {
        let mut fields = line.split('\t');
        let (Some(s), Some(n)) = (
            fields.next().and_then(|f: &str| f.chars().next()),
            fields.next(),
        ) else {
            continue;
        };
        let (Some((i, v, f)), Ok(n)) = (parts(s), n.trim().parse::<f64>()) else {
            continue;
        };
        counts.0[i] += n;
        counts.1[v] += n;
        counts.2[f] += n;
    }
    let log = |counts: &[f64]| {
        let total: f64 = counts.iter().sum();
        counts
            .iter()
            .map(|c| (c / total).ln())
            .collect::<Vec<f64>>()
    };
    let (i, v, f) = (log(&counts.0), log(&counts.1), log(&counts.2));
    JamoPrior {
        initial: i.try_into().unwrap_or([0.0; 19]),
        vowel: v.try_into().unwrap_or([0.0; 21]),
        last: f.try_into().unwrap_or([0.0; 28]),
    }
});

/// Words seen in Korean sentences (korean_words.txt).
static WORDS: PackLazy<SortedLines> = PackLazy::new(Language::Korean, || {
    SortedLines::new(pack::text(Language::Korean, "korean_words_sorted.txt"))
});

static VOCABULARY: PackLazy<SortedLines> = PackLazy::new(Language::Korean, || {
    SortedLines::new(pack::text(Language::Korean, "korean_vocab_sorted.txt"))
});

/// Where syllable probabilities come from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Tiers,
    Corpus,
    /// Whichever of the two rates a syllable higher.
    Max,
}

/// A range of lines in each word list (byte offsets): the words that start like some prefix.
#[derive(Clone, Copy)]
pub struct WordLines {
    words: (usize, usize),
    vocabulary: (usize, usize),
}

pub struct KoreanScore {
    /// Average log-probability per keystroke.
    pub average_log_probability: f64,
    pub composition: Composed,
}

pub struct KoreanModel {
    tiers: Cow<'static, HashMap<char, f64>>,
}

static SHARED_MAX: PackLazy<HashMap<char, f64>> =
    PackLazy::new(Language::Korean, || KoreanModel::build(Source::Max));

/// The initial consonants in syllable order, as the lone jamo a half-typed syllable shows.
const INITIALS: &str = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ";

/// (initial, vowel, final) indices of a precomposed syllable.
fn parts(c: char) -> Option<(usize, usize, usize)> {
    let i = (c as usize)
        .checked_sub('가' as usize)
        .filter(|i| *i < 11172)?;
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
    let Some(last) = prefix.chars().next_back() else {
        return true;
    };
    let head = &prefix[..prefix.len() - last.len_utf8()];
    word.strip_prefix(head)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| fits(last, c))
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
    /// The default (`Source::Max`) table is built once per process and shared by every engine.
    pub fn new(source: Source) -> Self {
        let tiers = if source == Source::Max {
            Cow::Borrowed(SHARED_MAX.get())
        } else {
            Cow::Owned(Self::build(source))
        };
        Self { tiers }
    }

    fn build(source: Source) -> HashMap<char, f64> {
        let mut map: HashMap<char, f64> = HashMap::new();
        if source != Source::Corpus {
            // later (more frequent) tiers overwrite earlier ones
            for (syllables, logp) in [
                (TIER3, TIER3_LOGP),
                (TIER2, TIER2_LOGP),
                (TIER1, TIER1_LOGP),
            ] {
                for s in syllables.chars().filter(|c| is_hangul_syllable(*c)) {
                    map.insert(s, logp);
                }
            }
        }
        if source != Source::Tiers {
            let counts: Vec<(char, f64)> = pack::text(Language::Korean, "korean_syllables.txt")
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
        map
    }

    /// A word that occurs in real Korean text (the composed Hangul, not the keys).
    pub fn is_known_word(&self, hangul: &str) -> bool {
        WORDS.get().contains(hangul)
    }

    /// A known word, or a vocabulary word (two syllables or more) with a particle/ending
    /// straight after it (서버를, 월드에서).
    pub fn is_known_word_or_inflected(&self, hangul: &str) -> bool {
        self.is_known_word(hangul) || self.is_inflected_vocabulary_word(hangul)
    }

    /// A vocabulary word (two syllables or more) with a particle/ending straight after it.
    pub fn is_inflected_vocabulary_word(&self, hangul: &str) -> bool {
        hangul.char_indices().skip(2).any(|(cut, _)| {
            let (head, tail) = hangul.split_at(cut);
            is_ending(tail) && self.is_vocabulary_word(head)
        })
    }

    /// A word in the wider vocabulary (loanwords like 월드, 서버), for English + Korean splits.
    pub fn is_vocabulary_word(&self, hangul: &str) -> bool {
        WORDS.get().contains(hangul) || VOCABULARY.get().contains(hangul)
    }

    /// Natural-log probability of a word (the composed Hangul): from its count in real text,
    /// or from its rank in the spoken vocabulary, whichever says it is likelier. None: unknown.
    pub fn word_log_probability(&self, hangul: &str) -> Option<f64> {
        let number = |line: &str| line.split('\t').nth(1)?.parse::<f64>().ok();
        let counted = WORDS
            .get()
            .get(hangul)
            .and_then(number)
            .map(|count| (count / WORDS_TOTAL).ln());
        let ranked = VOCABULARY
            .get()
            .get(hangul)
            .and_then(number)
            .map(|rank| -RANK_LOGP - RANK_SLOPE * rank.ln());
        match (counted, ranked) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    /// The lines of both word lists (counted words, vocabulary), to narrow by a word's syllables.
    pub fn word_lines() -> WordLines {
        WordLines {
            words: WORDS.get().span(),
            vocabulary: VOCABULARY.get().span(),
        }
    }

    /// The lines of `within` that start with `prefix`.
    pub fn narrow(&self, within: WordLines, prefix: &str) -> WordLines {
        WordLines {
            words: WORDS.get().prefix_range(within.words, prefix.as_bytes()),
            vocabulary: VOCABULARY
                .get()
                .prefix_range(within.vocabulary, prefix.as_bytes()),
        }
    }

    /// `word_log_probability` searching only `within`, lines that start like `hangul` (the
    /// word's first syllables, `narrow`ed): a walk over the syllables of a word asks for each
    /// longer word without searching the whole lists.
    pub fn word_log_probability_in(&self, within: WordLines, hangul: &str) -> Option<f64> {
        let number = |line: &str| line.split('\t').nth(1)?.parse::<f64>().ok();
        let counted = WORDS
            .get()
            .get_in(within.words, hangul.as_bytes())
            .and_then(number)
            .map(|count| (count / WORDS_TOTAL).ln());
        let ranked = VOCABULARY
            .get()
            .get_in(within.vocabulary, hangul.as_bytes())
            .and_then(number)
            .map(|rank| -RANK_LOGP - RANK_SLOPE * rank.ln());
        match (counted, ranked) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    /// The most commonly said word that continues `prefix` (autocomplete), where the syllable
    /// being typed may still be unfinished (안녀 → 안녕하세요); None under two syllables.
    pub fn complete(&self, prefix: &str) -> Option<String> {
        if prefix.chars().count() < 2 {
            return None;
        }
        let head = &prefix[..prefix.len() - prefix.chars().next_back()?.len_utf8()];
        let column = |line: &str| {
            line.split('\t')
                .nth(1)
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0)
        };
        let word = |line: &'static str| line.split('\t').next().unwrap_or(line);
        let candidates = |list: &'static SortedLines| {
            list.with_prefix(head)
                .filter(move |l| word(l) != prefix && continues(prefix, word(l)))
        };
        // Spoken-language ranking first (1 = most said), then the Tatoeba counts.
        if let Some(best) = candidates(VOCABULARY.get()).min_by_key(|l| column(l)) {
            return Some(word(best).to_string());
        }
        candidates(WORDS.get())
            .max_by_key(|l| column(l))
            .map(|l| word(l).to_string())
    }

    /// A syllable's log probability among all running Korean (a rare one is rare: 샬), and for
    /// one no text has shown the product of how common its initial, vowel and final are (a
    /// syllable made of rare parts is rarer: 럍 against 켜).
    pub fn plausible_log_probability(&self, syllable: char) -> f64 {
        match SYLLABLE_LOGP.get().get(&syllable) {
            Some(logp) => *logp,
            None => parts(syllable).map_or(UNKNOWN_SYLLABLE_LOGP, |(i, v, f)| {
                let prior = JAMO_PRIOR.get();
                prior.initial[i] + prior.vowel[v] + prior.last[f]
            }),
        }
    }

    pub fn syllable_log_probability(&self, syllable: char) -> f64 {
        self.tiers
            .get(&syllable)
            .copied()
            .unwrap_or(UNKNOWN_SYLLABLE_LOGP)
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
            && (units
                .iter()
                .all(|u| matches!(u, Unit::Jamo(j) if CHAT_JAMO.contains(*j)))
                || (units.len() == 2
                    && units
                        .iter()
                        .all(|u| matches!(u, Unit::Jamo(j) if is_consonant(*j)))));
        let mut total = 0.0;
        let mut strays = 0;
        for (i, unit) in units.iter().enumerate() {
            total += match unit {
                Unit::Syllable(s) => self.syllable_log_probability(*s),
                Unit::Jamo(j) => {
                    if all_chat {
                        CHAT_JAMO_LOGP
                    } else if partial && i == units.len() - 1 && is_consonant(*j) {
                        PENDING_CONSONANT_LOGP
                    } else {
                        strays += 1;
                        STRAY_JAMO_LOGP
                    }
                }
            };
        }
        let count = keys.chars().count();
        let mut average = total / count as f64;
        // A stray jamo inside a longer word (층ㄱ for cmdr) must not be averaged away by the
        // syllables around it.
        let at_edge = matches!(units.first(), Some(Unit::Jamo(_)))
            || matches!(units.last(), Some(Unit::Jamo(_)));
        if strays == 1 && at_edge && (STRAY_WORD_MIN_KEYS..=STRAY_WORD_MAX_KEYS).contains(&count) {
            average = average.min(STRAY_WORD_LOGP);
        }
        Some(KoreanScore {
            average_log_probability: average,
            composition,
        })
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
