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
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

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

/// Sum of the listed frequencies (the third column of english_levels.txt).
const FREQUENCY_TOTAL: f64 = 674_573_542.0;
/// A known word with no listed frequency costs this plus a step per level (rarer than most).
const UNLISTED_LOGP: f64 = 11.0;
const UNLISTED_LEVEL_STEP: f64 = 0.1;

pub struct EnglishModel {
    levels: SortedLines,
    /// The lines of the word list that start with each one letter and each two letters
    /// (26 + 26²), found the first time they are asked: the first steps of a walk would
    /// otherwise search the whole list. 0: not yet; else the end of the range + 1 over its start.
    starts: Vec<AtomicU64>,
}

/// Slots of `EnglishModel::starts`.
const START_SLOTS: usize = 26 + 26 * 26;

static SHARED: PackLazy<EnglishModel> = PackLazy::new(Language::English, || {
    EnglishModel::new(SortedLines::new(pack::text(
        Language::English,
        "english_levels.txt",
    )))
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

/// Log probability of letter `c` after `a b` (0 = word boundary).
fn trigram_term(a: usize, b: usize, c: usize) -> f64 {
    let n = SYMBOLS;
    let seen = count((a * n + b) * n + c);
    let context = count(n * n * n + a * n + b);
    f64::from(((seen + SMOOTHING) / (context + SMOOTHING * n as f32)).ln())
}

/// The level column of a line of the word list.
fn level_of(line: &str) -> Option<i32> {
    line.split('\t').nth(1)?.parse().ok()
}

/// The code of a lowercase letter in the trigram table (a = 1), 0 for none.
fn letter_code(letters: &[u8], from_end: usize) -> usize {
    letters
        .len()
        .checked_sub(from_end)
        .map_or(0, |i| usize::from(letters[i] - b'a') + 1)
}

/// The word list and the letter model followed along the letters of one stretch of keys, one
/// letter at a time. The stretches that start at the same key share all of it: a longer one
/// only adds a letter (a step in the word list over a few lines, a term of the letter
/// probability), where asking `level` about each of them starts from scratch.
#[derive(Default)]
pub struct Walk {
    /// Lowercase, as walked.
    letters: Vec<u8>,
    steps: Vec<Step>,
}

#[derive(Clone, Copy)]
struct Step {
    /// The lines of the word list that start with the letters so far (a byte range).
    lines: (u32, u32),
    /// Log probability of the letters so far, without the word end.
    open: f64,
}

/// The longest stretch walked.
pub const WALK_MAX: usize = 31;

impl Walk {
    pub fn len(&self) -> usize {
        self.letters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.letters.is_empty()
    }

    pub fn clear(&mut self) {
        self.letters.clear();
        self.steps.clear();
    }

    /// Keeps the letters that `keys` starts with too.
    fn follow(&mut self, keys: &[u8]) {
        let keep = self
            .letters
            .iter()
            .zip(keys)
            .take_while(|(a, b)| **a == b.to_ascii_lowercase())
            .count();
        self.letters.truncate(keep);
        self.steps.truncate(keep);
    }
}

impl EnglishModel {
    fn new(levels: SortedLines) -> Self {
        Self {
            levels,
            starts: (0..START_SLOTS).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    /// The lines that start with `letters` (lowercase), inside the lines `within` of their
    /// first letters but one.
    fn lines_of(&self, letters: &[u8], within: (usize, usize)) -> (usize, usize) {
        let code = |i: usize| usize::from(letters[i] - b'a');
        let slot = match letters.len() {
            1 => code(0),
            2 => 26 + code(0) * 26 + code(1),
            _ => return self.levels.prefix_range(within, letters),
        };
        let cell = &self.starts[slot];
        let packed = cell.load(Relaxed);
        if packed != 0 {
            return ((packed & 0xFFFF_FFFF) as usize, (packed >> 32) as usize - 1);
        }
        let (lo, hi) = self.levels.prefix_range(within, letters);
        cell.store((hi as u64 + 1) << 32 | lo as u64, Relaxed);
        (lo, hi)
    }

    /// Walks `keys` (letters, at most `WALK_MAX`), from where the walk already is.
    pub fn walk_to(&self, walk: &mut Walk, keys: &[u8]) {
        let keys = &keys[..keys.len().min(WALK_MAX)];
        walk.follow(keys);
        for &key in &keys[walk.len()..] {
            let letter = key.to_ascii_lowercase();
            let (before, last) = (&walk.letters[..], walk.steps.last());
            let (a, b) = (letter_code(before, 2), letter_code(before, 1));
            let open =
                last.map_or(0.0, |s| s.open) + trigram_term(a, b, usize::from(letter - b'a') + 1);
            let (lo, hi) = last.map_or_else(
                || {
                    let (lo, hi) = self.levels.span();
                    (lo as u32, hi as u32)
                },
                |s| s.lines,
            );
            walk.letters.push(letter);
            let lines = if lo == hi {
                (lo, hi)
            } else {
                let (lo, hi) = self.lines_of(&walk.letters, (lo as usize, hi as usize));
                (lo as u32, hi as u32)
            };
            walk.steps.push(Step { lines, open });
        }
    }

    /// The listed line of the first `len` letters, if the word list has them as a word.
    fn walk_line(&self, walk: &Walk, len: usize) -> Option<&'static str> {
        let (lo, hi) = walk.steps[len - 1].lines;
        self.levels
            .exact_in((lo as usize, hi as usize), &walk.letters[..len])
    }

    /// The level of the first `len` letters plus `extra`, if that is a listed word.
    fn walk_line_plus(&self, walk: &Walk, len: usize, extra: u8) -> Option<i32> {
        let mut word = [0u8; WALK_MAX + 1];
        word[..len].copy_from_slice(&walk.letters[..len]);
        word[len] = extra;
        let (lo, hi) = walk.steps[len - 1].lines;
        let lines = self
            .levels
            .prefix_range((lo as usize, hi as usize), &word[..=len]);
        level_of(self.levels.exact_in(lines, &word[..=len])?)
    }

    /// `level` of the first `len` letters of the walk (1 ≤ len ≤ walk.len()): the same answer,
    /// read off the steps already walked.
    pub fn walk_level(&self, walk: &Walk, len: usize) -> Option<i32> {
        if let Some(level) = self.walk_line(walk, len).and_then(level_of) {
            return Some(level);
        }
        let word = &walk.letters[..len];
        let mut best: Option<i32> = None;
        let mut consider = |level: Option<i32>| {
            if let Some(l) = level {
                best = Some(best.map_or(l, |b| b.min(l)));
            }
        };
        // Stems of 3+ letters: ths isn't the+s, ehs isn't eh+s.
        for suffix in ["s", "es", "ed", "d", "ing", "ly", "er", "ers", "est"] {
            if len > suffix.len() + 2 && word.ends_with(suffix.as_bytes()) {
                let stem = len - suffix.len();
                // used → use: a lone d follows a stem that ends in e.
                if suffix == "d" && word[stem - 1] != b'e' {
                    continue;
                }
                consider(self.walk_line(walk, stem).and_then(level_of));
                consider(self.walk_line_plus(walk, stem, b'e'));
                // running → run, stopped → stop
                if word[stem - 1] == word[stem - 2] {
                    consider(self.walk_line(walk, stem - 1).and_then(level_of));
                }
            }
        }
        // stories → story, denied → deny, heavier → heavy
        for suffix in ["ies", "ied", "ier", "iest", "iers"] {
            if len > suffix.len() + 1 && word.ends_with(suffix.as_bytes()) {
                consider(self.walk_line_plus(walk, len - suffix.len(), b'y'));
            }
        }
        best
    }

    /// `is_known_word` of the first `len` letters.
    pub fn walk_is_known(&self, walk: &Walk, len: usize) -> bool {
        self.walk_level(walk, len).is_some_and(|l| l <= KNOWN_LEVEL)
    }

    /// `is_common_word` of the first `len` letters.
    pub fn walk_is_common(&self, walk: &Walk, len: usize) -> bool {
        self.walk_level(walk, len)
            .is_some_and(|l| l <= COMMON_LEVEL)
    }

    /// `word_log_probability` of the first `len` letters.
    pub fn walk_word_log_probability(&self, walk: &Walk, len: usize) -> Option<f64> {
        let level = self.walk_level(walk, len).filter(|l| *l <= KNOWN_LEVEL)?;
        Some(Self::price(
            self.walk_line(walk, len)
                .and_then(|line| line.split('\t').nth(2)?.parse::<f64>().ok()),
            level,
        ))
    }

    /// `total_log_probability` (closed) or `prefix_log_probability` of the first `len` letters.
    pub fn walk_letters(&self, walk: &Walk, len: usize, closed: bool) -> f64 {
        let open = walk.steps[len - 1].open;
        if !closed {
            return open;
        }
        let letters = &walk.letters[..len];
        open + trigram_term(letter_code(letters, 2), letter_code(letters, 1), 0)
    }

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
        let (mut a, mut b, mut total) = (0usize, 0usize, 0f64);
        for byte in word.bytes().filter(u8::is_ascii_alphabetic) {
            let c = (byte.to_ascii_lowercase() - b'a' + 1) as usize;
            total += trigram_term(a, b, c);
            (a, b) = (b, c);
        }
        if closed {
            total += trigram_term(a, b, 0);
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
                    // used → use: a lone d follows a stem that ends in e (notificationd isn't one).
                    if suffix == "d" && !stem.ends_with('e') {
                        continue;
                    }
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

    /// Natural-log probability of a known word among all running English, from its listed
    /// frequency; a word with none (github, inflections) is priced by its level. None: not a word.
    pub fn word_log_probability(&self, token: &str) -> Option<f64> {
        let level = self.level(token).filter(|l| *l <= KNOWN_LEVEL)?;
        let listed = self
            .levels
            .get(&token.to_lowercase())
            .and_then(|line| line.split('\t').nth(2)?.parse::<f64>().ok());
        Some(Self::price(listed, level))
    }

    /// A word's log probability from its listed frequency, else from its level.
    fn price(listed: Option<f64>, level: i32) -> f64 {
        match listed {
            Some(count) => (count / FREQUENCY_TOTAL).ln(),
            None => -(UNLISTED_LOGP + UNLISTED_LEVEL_STEP * f64::from(level)),
        }
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

    /// The walk answers exactly what the one-shot calls do: levels (also of inflections),
    /// word probabilities and the letter probabilities, bit for bit.
    #[test]
    fn walking_equals_asking_afresh() {
        let model = EnglishModel::shared();
        let mut samples: Vec<String> = [
            "nodejs",
            "qwertyuiop",
            "running",
            "stopped",
            "stories",
            "denied",
            "heavier",
            "used",
            "notificationd",
            "githubs",
            "kakaotalk",
            "aaa",
            "zzzz",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        for line in model.levels.with_prefix("").step_by(41) {
            let word = line.split('\t').next().unwrap_or("");
            if !word.bytes().all(|b| b.is_ascii_lowercase()) || word.len() > WALK_MAX - 5 {
                continue;
            }
            samples.push(word.to_string());
            for suffix in [
                "s", "es", "ed", "d", "ing", "ly", "er", "ers", "est", "ies", "ied",
            ] {
                samples.push(format!("{word}{suffix}"));
                let cut = &word[..word.len() - 1];
                samples.push(format!("{cut}{suffix}"));
                let last = &word[word.len() - 1..];
                samples.push(format!("{word}{last}{suffix}"));
            }
        }
        assert!(samples.len() > 1000);
        let mut walk = Walk::default();
        for word in &samples {
            // Capitals are walked as the lowercase letters.
            let typed: String = word
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    if i % 3 == 0 {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    }
                })
                .collect();
            walk.clear();
            model.walk_to(&mut walk, typed.as_bytes());
            for len in 1..=walk.len() {
                let part = &typed[..len];
                assert_eq!(
                    model.walk_level(&walk, len),
                    model.level(part),
                    "{typed} {len}"
                );
                assert_eq!(
                    model.walk_is_known(&walk, len),
                    model.is_known_word(part),
                    "{typed} {len}"
                );
                assert_eq!(
                    model.walk_is_common(&walk, len),
                    model.is_common_word(part),
                    "{typed} {len}"
                );
                assert_eq!(
                    model
                        .walk_word_log_probability(&walk, len)
                        .map(f64::to_bits),
                    model.word_log_probability(part).map(f64::to_bits),
                    "{typed} {len}"
                );
                for closed in [true, false] {
                    let fresh = if closed {
                        model.total_log_probability(part)
                    } else {
                        model.prefix_log_probability(part)
                    };
                    assert_eq!(
                        model.walk_letters(&walk, len, closed).to_bits(),
                        fresh.to_bits(),
                        "{typed} {len}"
                    );
                }
            }
        }
        // Following a changed token keeps what is shared and walks the rest.
        walk.clear();
        model.walk_to(&mut walk, b"githubdp");
        model.walk_to(&mut walk, b"gitlabxyz");
        let mut fresh = Walk::default();
        model.walk_to(&mut fresh, b"gitlabxyz");
        for len in 1..=fresh.len() {
            assert_eq!(model.walk_level(&walk, len), model.walk_level(&fresh, len));
            assert_eq!(
                model.walk_letters(&walk, len, true).to_bits(),
                model.walk_letters(&fresh, len, true).to_bits()
            );
        }
    }

    #[test]
    fn empty_word_list_knows_nothing() {
        let model = EnglishModel::new(SortedLines::new(""));
        assert!(!model.is_known_word("hello"));
        assert_eq!(model.word_count(), 0);
    }
}
