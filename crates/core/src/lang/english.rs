//! Character-trigram language model plus a word list with how common each word is, from
//! SCOWL (word<TAB>level<TAB>frequency, level 10 = most common … 60). Port of EnglishModel.swift.
//!
//! The original trains the letter trigrams on macOS's /usr/share/dict/words at runtime and
//! builds its word map from the list. Here build.rs does both (trained on Webster's 2nd, public
//! domain) and the DLL carries the result as read-only data: see build.rs.

use crate::decide::engine::Language;
use crate::lang::pack::{self, PackLazy};
use crate::lang::sorted::SortedLines;
use std::cell::RefCell;

thread_local! {
    /// (word list identity, lowercased word, level) of the last `level` call.
    static LAST_LEVEL: RefCell<((usize, usize), String, Option<i32>)> =
        const { RefCell::new(((0, 0), String::new(), None)) };
}

static TRIGRAMS: PackLazy<&'static [u8]> = PackLazy::new(Language::English, || {
    pack::bytes(Language::English, "trigrams.bin")
});

// Symbols: 0 = word boundary, 1..=26 = a..z
const SYMBOLS: usize = 27;
const SMOOTHING: f32 = 0.5;

/// Words up to this level count as English words (a score floor, protection from kana).
pub const KNOWN_LEVEL: i32 = 50;
/// Words up to this level are everyday English (you, sense, machine, for, got).
pub const COMMON_LEVEL: i32 = 20;
/// Level given to the extra tech/slang words SCOWL lacks: known, not "common".
pub const EXTRA_LEVEL: i32 = 35;

pub struct EnglishModel {
    levels: SortedLines,
}

static SHARED: PackLazy<EnglishModel> = PackLazy::new(Language::English, || EnglishModel {
    levels: SortedLines::new(pack::text(Language::English, "english_levels.txt")),
});

/// [^, ^, letters..., $] with ^ and $ both encoded as 0.
fn encode(word: &str) -> Vec<usize> {
    let mut codes = vec![0, 0];
    codes.extend(
        word.bytes()
            .filter(u8::is_ascii_lowercase)
            .map(|b| (b - b'a' + 1) as usize),
    );
    codes.push(0);
    codes
}

/// Counts are stored trigrams first (27³), then contexts (27²). Missing data counts 0.
fn count(index: usize) -> f32 {
    TRIGRAMS
        .get()
        .get(index * 4..index * 4 + 4)
        .map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

impl EnglishModel {
    /// One model per process, built lazily.
    pub fn shared() -> &'static EnglishModel {
        SHARED.get()
    }

    pub fn word_count(&self) -> usize {
        self.levels.len()
    }

    /// Average natural-log probability per symbol (letters + end marker).
    /// Real English words land around -1.8…-2.4; keyboard mash around -3.5 or lower.
    pub fn average_log_probability(&self, token: &str) -> f64 {
        let n = SYMBOLS;
        let codes = encode(&token.to_lowercase());
        if codes.len() <= 3 {
            return -10.0;
        }
        let mut total = 0f64;
        for i in 2..codes.len() {
            let c = count((codes[i - 2] * n + codes[i - 1]) * n + codes[i]);
            let t = count(n * n * n + codes[i - 2] * n + codes[i - 1]);
            total += f64::from(((c + SMOOTHING) / (t + SMOOTHING * n as f32)).ln());
        }
        total / (codes.len() - 2) as f64
    }

    /// Total natural-log probability of the letters of `word` (the sum behind
    /// `average_log_probability`, without its allocations; other characters are skipped).
    pub fn total_log_probability(&self, word: &str) -> f64 {
        self.letters_log_probability(word, true)
    }

    /// The same without the word end: the letters of a word still being typed (edi → editor).
    pub fn prefix_log_probability(&self, word: &str) -> f64 {
        self.letters_log_probability(word, false)
    }

    fn letters_log_probability(&self, word: &str, closed: bool) -> f64 {
        let n = SYMBOLS;
        let term = |a: usize, b: usize, c: usize| {
            let seen = count((a * n + b) * n + c);
            let context = count(n * n * n + a * n + b);
            f64::from(((seen + SMOOTHING) / (context + SMOOTHING * n as f32)).ln())
        };
        let (mut a, mut b, mut total) = (0usize, 0usize, 0f64);
        for byte in word.bytes().filter(u8::is_ascii_alphabetic) {
            let c = (byte.to_ascii_lowercase() - b'a' + 1) as usize;
            total += term(a, b, c);
            (a, b) = (b, c);
        }
        if closed {
            total += term(a, b, 0);
        }
        total
    }

    fn listed(&self, word: &str) -> Option<i32> {
        self.levels.get(word)?.split('\t').nth(1)?.parse().ok()
    }

    /// The word's level, also for regular inflections of a listed word (None: not a word).
    /// One decision asks about the same keys several times, so the last answer is remembered.
    pub fn level(&self, token: &str) -> Option<i32> {
        let w: String = token
            .to_lowercase()
            .chars()
            .filter(|c| *c != '\'')
            .collect();
        let me = self.levels.id();
        let hit = LAST_LEVEL.with(|c| {
            let c = c.borrow();
            (c.0 == me && c.1 == w).then_some(c.2)
        });
        if let Some(level) = hit {
            return level;
        }
        let level = self.compute_level(&w);
        LAST_LEVEL.with(|c| *c.borrow_mut() = (me, w, level));
        level
    }

    fn compute_level(&self, w: &str) -> Option<i32> {
        if let Some(l) = self.listed(w) {
            return Some(l);
        }
        let len = w.chars().count();
        let mut best: Option<i32> = None;
        let mut buf = String::new();
        let mut consider = |stem: &str| {
            if let Some(l) = self.listed(stem) {
                best = Some(best.map_or(l, |b| b.min(l)));
            }
        };
        // Stems of 3+ letters: ths isn't the+s, ehs isn't eh+s.
        for suffix in ["s", "es", "ed", "d", "ing", "ly", "er", "ers", "est"] {
            if len > suffix.len() + 2 {
                if let Some(stem) = w.strip_suffix(suffix) {
                    consider(stem);
                    buf.clear();
                    buf.push_str(stem);
                    buf.push('e');
                    consider(&buf);
                    // running → run, stopped → stop
                    let mut rev = stem.chars().rev();
                    if let (Some(last), Some(prev)) = (rev.next(), rev.next()) {
                        if last == prev {
                            consider(&stem[..stem.len() - last.len_utf8()]);
                        }
                    }
                }
            }
        }
        // stories → story, denied → deny, heavier → heavy
        for suffix in ["ies", "ied", "ier", "iest", "iers"] {
            if len > suffix.len() + 1 {
                if let Some(stem) = w.strip_suffix(suffix) {
                    buf.clear();
                    buf.push_str(stem);
                    buf.push('y');
                    consider(&buf);
                }
            }
        }
        best
    }

    /// The word most often said that continues `prefix` (autocomplete); None under three
    /// letters or when `prefix` is already an everyday word.
    pub fn complete(&self, prefix: &str) -> Option<String> {
        let prefix = prefix.to_lowercase();
        if prefix.chars().count() < 3 || self.is_common_word(&prefix) {
            return None;
        }
        let column = |line: &str, i: usize| {
            line.split('\t')
                .nth(i)
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0)
        };
        let best = self
            .levels
            .with_prefix(&prefix)
            .filter(|l| {
                column(l, 1) <= EXTRA_LEVEL as u64 && l.split('\t').next() != Some(prefix.as_str())
            })
            .max_by_key(|l| {
                (
                    column(l, 2),
                    std::cmp::Reverse(column(l, 1)),
                    std::cmp::Reverse(l.len()),
                )
            })?;
        best.split('\t').next().map(String::from)
    }

    /// A word people use (SCOWL level ≤ 50, or one of the extra words).
    pub fn is_known_word(&self, token: &str) -> bool {
        self.level(token).is_some_and(|l| l <= KNOWN_LEVEL)
    }

    /// Everyday English (level ≤ 20): kept English even right after Korean.
    pub fn is_common_word(&self, token: &str) -> bool {
        self.level(token).is_some_and(|l| l <= COMMON_LEVEL)
    }
}

#[cfg(test)]
mod empty_data {
    use super::*;

    #[test]
    fn out_of_range_trigrams_count_zero() {
        assert_eq!(count(usize::MAX / 8), 0.0);
    }

    #[test]
    fn total_matches_the_average() {
        let model = EnglishModel::shared();
        for word in ["hello", "kakaotalk", "x", "Slack", "mongodb"] {
            let letters = word.len() as f64 + 1.0;
            let total = model.total_log_probability(word);
            assert!(
                (total - model.average_log_probability(word) * letters).abs() < 1e-9,
                "{word}"
            );
        }
    }

    #[test]
    fn empty_word_list_knows_nothing() {
        let model = EnglishModel {
            levels: SortedLines::new(""),
        };
        assert!(!model.is_known_word("hello"));
        assert_eq!(model.word_count(), 0);
    }
}
