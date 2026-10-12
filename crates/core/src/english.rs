//! Character-trigram language model plus a word list with how common each word is, from
//! SCOWL (word<TAB>level, 10 = most common … 60). Port of EnglishModel.swift.
//!
//! The original trains the letter trigrams on macOS's /usr/share/dict/words at runtime and
//! builds its word map from the list. Here build.rs does both (trained on Webster's 2nd, public
//! domain) and the DLL carries the result as read-only data: see build.rs.

use std::sync::LazyLock;

use crate::sorted::SortedLines;

const LEVELS: &str = include_str!(concat!(env!("OUT_DIR"), "/english_levels.txt"));
const TRIGRAMS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/trigrams.bin"));

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

static SHARED: LazyLock<EnglishModel> = LazyLock::new(|| EnglishModel { levels: SortedLines::new(LEVELS) });

/// [^, ^, letters..., $] with ^ and $ both encoded as 0.
fn encode(word: &str) -> Vec<usize> {
    let mut codes = vec![0, 0];
    codes.extend(word.bytes().filter(u8::is_ascii_lowercase).map(|b| (b - b'a' + 1) as usize));
    codes.push(0);
    codes
}

/// Counts are stored trigrams first (27³), then contexts (27²).
fn count(index: usize) -> f32 {
    let b = &TRIGRAMS[index * 4..index * 4 + 4];
    f32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

impl EnglishModel {
    /// One model per process, built lazily.
    pub fn shared() -> &'static EnglishModel {
        &SHARED
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

    fn listed(&self, word: &str) -> Option<i32> {
        self.levels.get(word)?.split('\t').nth(1)?.parse().ok()
    }

    /// The word's level, also for regular inflections of a listed word (None: not a word).
    pub fn level(&self, token: &str) -> Option<i32> {
        let w: String = token.to_lowercase().chars().filter(|c| *c != '\'').collect();
        if let Some(l) = self.listed(&w) {
            return Some(l);
        }
        let len = w.chars().count();
        let mut best: Option<i32> = None;
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
                    consider(&format!("{stem}e"));
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
                    consider(&format!("{stem}y"));
                }
            }
        }
        best
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
