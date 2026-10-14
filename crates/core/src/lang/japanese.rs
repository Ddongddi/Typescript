//! Kana trigram model for judging how Japanese a romaji string reads. Counts come
//! from Japanese sentences (Tatoeba, CC-BY 2.0 FR) turned into the romaji people
//! type and back into kana with `RomajiComposer`; tools/build_kana_table.py packs
//! the counted table.
//!
//! Clean romaji isn't enough: "hello" composes to へっぉ, which this model scores
//! as very un-Japanese (っぉ essentially never occurs).
//! Port of JapaneseModel.swift.

use std::collections::{HashMap, HashSet};

use crate::decide::engine::Language;
use crate::lang::pack::{self, PackLazy};
use crate::lang::romaji::{ComposeResult, RomajiComposer};

const MAGIC: &[u8; 8] = b"TSKANA01";
const STRAY_LETTER_LOGP: f64 = -13.0;
const PENDING_LOGP: f64 = -3.5;

pub struct JapaneseScore {
    /// Average log-probability per keystroke, comparable with the English and Korean models.
    pub average_log_probability: f64,
    pub composition: ComposeResult,
}

pub struct JapaneseModel {
    trigrams: HashMap<(char, char, char), f32>,
    bigrams: HashMap<(char, char), f32>,
    unigrams: HashMap<char, f32>,
    bigram_contexts: HashMap<char, f32>,
    trigram_contexts: HashMap<(char, char), f32>,
    total: f32,
    vocabulary: f32,
}

static SHARED: PackLazy<JapaneseModel> = PackLazy::new(Language::Japanese, || {
    JapaneseModel::new(pack::bytes(Language::Japanese, "kana_trigrams.bin"))
});

/// The packed table's character list, or `None` when the bytes are not that table.
fn table(bytes: &[u8]) -> Option<Vec<char>> {
    if bytes.len() < 12 || &bytes[..8] != MAGIC {
        return None;
    }
    let n_chars = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
    if n_chars == 0 || n_chars > 256 || 12 + 2 * n_chars + 5 > bytes.len() {
        return None;
    }
    let mut chars = Vec::with_capacity(n_chars);
    for i in 0..n_chars {
        let at = 12 + 2 * i;
        chars.push(char::from_u32(
            u16::from_le_bytes([bytes[at], bytes[at + 1]]) as u32,
        )?);
    }
    Some(chars)
}

/// Feeds every record of the packed table to `add` and returns the total count; `None` when a
/// record is cut short or a code is outside the character table. Shared by `new` and `valid`,
/// so the check and the build cannot drift apart.
fn fold_records(bytes: &[u8], mut add: impl FnMut(char, char, char, f32)) -> Option<f32> {
    let chars = table(bytes)?;
    let at = 12 + 2 * chars.len();
    if bytes.len() == at || !(bytes.len() - at).is_multiple_of(5) {
        return None;
    }
    let mut total = 0f32;
    let (records, _) = bytes[at..].as_chunks::<5>();
    for record in records {
        let (a, b, c) = (record[0] as usize, record[1] as usize, record[2] as usize);
        if a >= chars.len() || b >= chars.len() || c >= chars.len() {
            return None;
        }
        let count = u16::from_le_bytes([record[3], record[4]]) as f32;
        add(chars[a], chars[b], chars[c], count);
        total += count;
    }
    Some(total)
}

impl JapaneseModel {
    /// One model per process, built lazily (Japanese is off by default; only a session with
    /// Japanese enabled pays for this).
    pub fn shared() -> &'static JapaneseModel {
        SHARED.get()
    }

    /// The bytes are a table the model can be built from: the packed layout, every code
    /// inside the character table, at least one count (what `new` asserts). A pack is only
    /// registered when this holds, so a damaged file costs the Japanese pack, not the text
    /// service.
    pub fn valid(bytes: &[u8]) -> bool {
        fold_records(bytes, |_, _, _, _| {}).is_some_and(|total| total > 0.0)
    }

    fn new(bytes: &'static [u8]) -> Self {
        let mut trigrams: HashMap<(char, char, char), f32> = HashMap::new();
        let mut bigrams: HashMap<(char, char), f32> = HashMap::new();
        let mut unigrams: HashMap<char, f32> = HashMap::new();
        let mut bigram_contexts: HashMap<char, f32> = HashMap::new();
        let mut trigram_contexts: HashMap<(char, char), f32> = HashMap::new();
        let mut symbols: HashSet<char> = HashSet::new();
        let total = fold_records(bytes, |a, b, c, count| {
            *trigrams.entry((a, b, c)).or_default() += count;
            *trigram_contexts.entry((a, b)).or_default() += count;
            *bigrams.entry((b, c)).or_default() += count;
            *bigram_contexts.entry(b).or_default() += count;
            *unigrams.entry(c).or_default() += count;
            symbols.extend([a, b, c]);
        })
        .expect("data/kana_trigrams.bin is unreadable");
        assert!(total > 0.0, "data/kana_trigrams.bin has no counts");
        Self {
            trigrams,
            bigrams,
            unigrams,
            bigram_contexts,
            trigram_contexts,
            total,
            vocabulary: symbols.len() as f32,
        }
    }

    /// Interpolated trigram probability of `c` after `a b`.
    fn probability(&self, a: char, b: char, c: char) -> f64 {
        let p1 =
            (self.unigrams.get(&c).copied().unwrap_or(0.0) + 1.0) / (self.total + self.vocabulary);
        let p2 = self.bigram_contexts.get(&b).map_or(0.0, |context| {
            self.bigrams.get(&(b, c)).copied().unwrap_or(0.0) / context
        });
        let p3 = self.trigram_contexts.get(&(a, b)).map_or(0.0, |context| {
            self.trigrams.get(&(a, b, c)).copied().unwrap_or(0.0) / context
        });
        f64::from(0.6 * p3 + 0.3 * p2 + 0.1 * p1)
    }

    /// Sum of log-probabilities of the kana (and the end of word unless `partial`).
    pub fn log_probability(&self, kana: &str, partial: bool) -> f64 {
        let mut seq: Vec<char> = vec!['^', '^'];
        seq.extend(kana.chars());
        if !partial {
            seq.push('$');
        }
        let mut sum = 0.0;
        for i in 2..seq.len() {
            sum += self.probability(seq[i - 2], seq[i - 1], seq[i]).ln();
        }
        sum
    }

    pub fn score(&self, keys: &str, partial: bool) -> Option<JapaneseScore> {
        if keys.is_empty()
            || !keys.chars().all(|c| {
                c.is_ascii() && (c.is_alphabetic() || RomajiComposer::is_in_phrase_symbol(c))
            })
        {
            return None;
        }
        let composition = RomajiComposer::compose(keys, partial);
        let keep = composition.text.chars().count() - composition.pending.chars().count();
        let kana: String = composition
            .text
            .chars()
            .take(keep)
            .filter(|c| !c.is_ascii() && !RomajiComposer::is_punctuation(*c))
            .collect();
        let mut total = self.log_probability(&kana, partial);
        total += composition.stray_letters as f64 * STRAY_LETTER_LOGP;
        if !composition.pending.is_empty() {
            total += PENDING_LOGP;
        }
        Some(JapaneseScore {
            average_log_probability: total / keys.chars().count() as f64,
            composition,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_the_model_cannot_read_is_refused_by_valid() {
        fn packed(records: &[(&str, u16)]) -> Vec<u8> {
            let mut chars: Vec<char> = records.iter().flat_map(|(f, _)| f.chars()).collect();
            chars.sort_unstable();
            chars.dedup();
            let mut out = Vec::new();
            out.extend(MAGIC);
            out.extend((chars.len() as u16).to_le_bytes());
            out.extend([0u8; 2]);
            for c in &chars {
                out.extend((*c as u16).to_le_bytes());
            }
            for (field, count) in records {
                out.extend(
                    field
                        .chars()
                        .map(|c| chars.binary_search(&c).unwrap() as u8),
                );
                out.extend(count.to_le_bytes());
            }
            out
        }
        let good = packed(&[("^^あ", 11602), ("^^い", 6284)]);
        assert!(JapaneseModel::valid(&good));
        let mut outside = good.clone();
        let last = outside.len() - 5;
        outside[last] = 200;
        let mut not_a_character = packed(&[("^^あ", 7)]);
        not_a_character[12] = 0x00;
        not_a_character[13] = 0xD8;
        for (what, bytes) in [
            ("empty", Vec::new()),
            ("no records", packed(&[])),
            ("a bad magic", {
                let mut b = good.clone();
                b[0] = b'X';
                b
            }),
            ("every count zero", packed(&[("^^あ", 0)])),
            (
                "a record that is cut short",
                good[..good.len() - 1].to_vec(),
            ),
            ("a code outside the character table", outside),
            ("a code point that is not a character", not_a_character),
        ] {
            assert!(!JapaneseModel::valid(&bytes), "{what} must fail valid()");
        }
    }

    #[test]
    fn loads_and_scores() {
        let m = JapaneseModel::shared();
        let good = m.score("konnichiha", false).expect("composes");
        let bad = m.score("hello", false).expect("composes");
        assert!(
            good.average_log_probability > bad.average_log_probability,
            "{} vs {}",
            good.average_log_probability,
            bad.average_log_probability
        );
        // Non-letters outside the phrase symbols are not judged at all.
        assert!(m.score("dk1sud", false).is_none());
        // Garbled kana is penalised through the stray letters.
        let garbled = m.score("dkssud", false).expect("composes");
        assert!(garbled.composition.stray_letters > 0);
    }
}
