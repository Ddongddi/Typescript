//! Kana trigram model for judging how Japanese a romaji string reads. Counts come
//! from Japanese sentences (Tatoeba, CC-BY 2.0 FR) turned into the romaji people
//! type and back into kana with `RomajiComposer` (tools/build_kana_model.py).
//!
//! Clean romaji isn't enough: "hello" composes to へっぉ, which this model scores
//! as very un-Japanese (っぉ essentially never occurs).
//! Port of JapaneseModel.swift.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use crate::lang::romaji::{ComposeResult, RomajiComposer};

#[cfg(feature = "japanese")]
const KANA_TRIGRAMS: &str = include_str!("../../data/kana_trigrams.txt");
#[cfg(not(feature = "japanese"))]
const KANA_TRIGRAMS: &str = ""; // built without Japanese: never read, see `crate::JAPANESE`

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

static SHARED: LazyLock<JapaneseModel> = LazyLock::new(|| JapaneseModel::new(KANA_TRIGRAMS));

impl JapaneseModel {
    /// One model per process, built lazily (Japanese is off by default; only a session with
    /// Japanese enabled pays for this).
    pub fn shared() -> &'static JapaneseModel {
        &SHARED
    }

    fn new(text: &str) -> Self {
        let mut trigrams: HashMap<(char, char, char), f32> = HashMap::new();
        let mut bigrams: HashMap<(char, char), f32> = HashMap::new();
        let mut unigrams: HashMap<char, f32> = HashMap::new();
        let mut bigram_contexts: HashMap<char, f32> = HashMap::new();
        let mut trigram_contexts: HashMap<(char, char), f32> = HashMap::new();
        let mut symbols: HashSet<char> = HashSet::new();
        let mut total = 0f32;
        for line in text.lines() {
            let mut parts = line.split('\t');
            let (Some(field), Some(count)) = (parts.next(), parts.next()) else {
                continue;
            };
            if parts.next().is_some() {
                continue;
            }
            let Ok(count) = count.trim().parse::<f32>() else {
                continue;
            };
            let chars: Vec<char> = field.chars().collect();
            if chars.len() != 3 {
                continue;
            }
            let (a, b, c) = (chars[0], chars[1], chars[2]);
            *trigrams.entry((a, b, c)).or_default() += count;
            *trigram_contexts.entry((a, b)).or_default() += count;
            *bigrams.entry((b, c)).or_default() += count;
            *bigram_contexts.entry(b).or_default() += count;
            *unigrams.entry(c).or_default() += count;
            total += count;
            symbols.extend(chars);
        }
        assert!(total > 0.0, "data/kana_trigrams.txt is empty or unreadable");
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

#[cfg(all(test, feature = "japanese"))]
mod tests {
    use super::*;

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
