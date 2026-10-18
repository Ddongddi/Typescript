//! Lab-only linear-chain CRF: raw key n-grams and learned tag transitions, no dictionaries,
//! unit lists, word-shape bonuses or confidence thresholds. It is not a production fallback.

use std::{path::Path, sync::Arc};
use tacit_core::lang::hangul::{DubeolsikLayout, HangulComposer};
use tacit_core::{Decision, DecisionContext, DecisionEngine, Language};

const BINS: usize = 1 << 14;
const SIZE: usize = 8 + 2 * BINS; // start, end, transition, then emission weights
const MAGIC: &[u8; 8] = b"TSENCRF1";

pub struct Model {
    weights: Vec<f64>,
}

struct Sample {
    keys: String,
    expected: String,
    labels: Vec<usize>,
    previous: Option<Language>,
    features: Vec<Vec<usize>>,
}

fn add(a: f64, b: f64) -> f64 {
    let m = a.max(b);
    if m == f64::NEG_INFINITY {
        m
    } else {
        m + ((a - m).exp() + (b - m).exp()).ln()
    }
}

fn features(keys: &[u8], previous: Option<Language>) -> Vec<Vec<usize>> {
    (0..keys.len())
        .map(|i| {
            let mut out = Vec::with_capacity(20);
            let mut push = |bytes: &[u8]| {
                let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
                    (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
                });
                out.push(hash as usize % BINS);
            };
            push(&[
                0,
                previous.map_or(0, |l| {
                    if l.sentence_language() == Language::Korean {
                        2
                    } else {
                        1
                    }
                }),
            ]);
            // Symmetric local context is restricted to the keys already typed in this call.
            for offset in -2isize..=2 {
                let start = i as isize + offset;
                for n in 1..=3 {
                    if start < 0 || start as usize + n > keys.len() {
                        continue;
                    }
                    let mut gram = [0u8; 5];
                    gram[0] = (offset + 3) as u8;
                    gram[1] = n as u8;
                    gram[2..2 + n].copy_from_slice(&keys[start as usize..start as usize + n]);
                    push(&gram[..2 + n]);
                }
            }
            if i == 0 {
                push(&[8, 0]);
            }
            if i + 1 == keys.len() {
                push(&[8, 1]);
            }
            out
        })
        .collect()
}

fn render(keys: &str, tags: &[usize]) -> String {
    let mut out = String::new();
    let mut start = 0;
    while start < keys.len() {
        let mut end = start + 1;
        while end < keys.len() && tags[end] == tags[start] {
            end += 1;
        }
        let part = &keys[start..end];
        if tags[start] == 1 {
            out.push_str(
                &HangulComposer::compose(part).map_or_else(|| part.to_string(), |c| c.text()),
            );
        } else {
            out.push_str(part);
        }
        start = end;
    }
    out
}

impl Model {
    fn emissions(&self, keys: &[u8], fs: &[Vec<usize>], on: [bool; 2]) -> Vec<[f64; 2]> {
        fs.iter()
            .zip(keys)
            .map(|(f, key)| {
                std::array::from_fn(|tag| {
                    if !on[tag] || (tag == 1 && DubeolsikLayout::jamo(*key as char).is_none()) {
                        f64::NEG_INFINITY
                    } else {
                        f.iter().map(|i| self.weights[8 + 2 * i + tag]).sum()
                    }
                })
            })
            .collect()
    }

    fn forward(&self, e: &[[f64; 2]]) -> (Vec<[f64; 2]>, f64) {
        let mut a = vec![[0.0; 2]; e.len()];
        for t in 0..2 {
            a[0][t] = self.weights[t] + e[0][t];
        }
        for i in 1..e.len() {
            for t in 0..2 {
                a[i][t] = e[i][t]
                    + add(
                        a[i - 1][0] + self.weights[4 + t],
                        a[i - 1][1] + self.weights[6 + t],
                    );
            }
        }
        let last = a.last().unwrap();
        let z = add(last[0] + self.weights[2], last[1] + self.weights[3]);
        (a, z)
    }

    fn score(&self, e: &[[f64; 2]], tags: &[usize]) -> f64 {
        self.weights[tags[0]]
            + self.weights[2 + tags[tags.len() - 1]]
            + tags
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    e[i][*t]
                        + if i == 0 {
                            0.0
                        } else {
                            self.weights[4 + 2 * tags[i - 1] + *t]
                        }
                })
                .sum::<f64>()
    }

    fn decode(
        &self,
        keys: &str,
        previous: Option<Language>,
        on: [bool; 2],
    ) -> Option<(Vec<usize>, f64)> {
        if keys.is_empty() || !keys.is_ascii() {
            return None;
        }
        let e = self.emissions(keys.as_bytes(), &features(keys.as_bytes(), previous), on);
        let (_, z) = self.forward(&e);
        if !z.is_finite() {
            return None;
        }
        let mut best = [self.weights[0] + e[0][0], self.weights[1] + e[0][1]];
        let mut from = vec![[0; 2]; e.len()];
        for i in 1..e.len() {
            let mut next = [0.0; 2];
            for t in 0..2 {
                let p = usize::from(best[1] + self.weights[6 + t] > best[0] + self.weights[4 + t]);
                from[i][t] = p;
                next[t] = best[p] + self.weights[4 + 2 * p + t] + e[i][t];
            }
            best = next;
        }
        let mut tag = usize::from(best[1] + self.weights[3] > best[0] + self.weights[2]);
        let score = best[tag] + self.weights[2 + tag];
        let mut tags = vec![0; e.len()];
        for i in (0..e.len()).rev() {
            tags[i] = tag;
            tag = from[i][tag];
        }
        Some((tags, (score - z).exp().min(1.0)))
    }

    fn train(&mut self, sample: &Sample, accumulated: &mut [f64]) -> f64 {
        let e = self.emissions(sample.keys.as_bytes(), &sample.features, [true, true]);
        let (a, z) = self.forward(&e);
        let loss = z - self.score(&e, &sample.labels);
        let n = e.len();
        let mut b = vec![[0.0; 2]; n];
        b[n - 1] = [self.weights[2], self.weights[3]];
        for i in (0..n - 1).rev() {
            for t in 0..2 {
                b[i][t] = add(
                    self.weights[4 + 2 * t] + e[i + 1][0] + b[i + 1][0],
                    self.weights[5 + 2 * t] + e[i + 1][1] + b[i + 1][1],
                );
            }
        }
        let mut delta = [0.0; 8];
        for i in 0..n {
            for t in 0..2 {
                let gold = f64::from(sample.labels[i] == t);
                let posterior = (a[i][t] + b[i][t] - z).exp();
                if i == 0 {
                    delta[t] += gold - posterior;
                }
                if i + 1 == n {
                    delta[2 + t] += gold - posterior;
                }
                if i > 0 {
                    for p in 0..2 {
                        delta[4 + 2 * p + t] +=
                            f64::from(sample.labels[i - 1] == p && sample.labels[i] == t)
                                - (a[i - 1][p] + self.weights[4 + 2 * p + t] + e[i][t] + b[i][t]
                                    - z)
                                    .exp();
                    }
                }
            }
        }
        let update = |weights: &mut [f64], accumulated: &mut [f64], i: usize, gradient: f64| {
            accumulated[i] += gradient * gradient;
            weights[i] += 0.1 * gradient / (accumulated[i] + 1e-8).sqrt();
        };
        for i in 0..n {
            for t in 0..2 {
                let g = f64::from(sample.labels[i] == t) - (a[i][t] + b[i][t] - z).exp();
                for f in &sample.features[i] {
                    update(&mut self.weights, accumulated, 8 + 2 * f + t, g);
                }
            }
        }
        for (i, g) in delta.into_iter().enumerate() {
            update(&mut self.weights, accumulated, i, g);
        }
        loss
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len()
            != (MAGIC.len() + SIZE * 8) as u64
        {
            return Err("invalid enko model size".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() != MAGIC.len() + SIZE * 8 || &bytes[..8] != MAGIC {
            return Err("invalid enko model header/size".into());
        }
        let weights: Vec<_> = bytes[8..]
            .chunks_exact(8)
            .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
            .collect();
        if !weights.iter().all(|w| w.is_finite()) {
            return Err("non-finite enko model weight".into());
        }
        Ok(Self { weights })
    }
}

pub struct Engine {
    pub model: Arc<Model>,
    pub on: [bool; 2],
}

impl DecisionEngine for Engine {
    fn decide(&self, keys: &str, context: &DecisionContext) -> Decision {
        let (tags, confidence) = self
            .model
            .decode(keys, context.previous_language, self.on)
            .unwrap_or_else(|| (vec![0; keys.len()], 0.0));
        let en = tags.contains(&0);
        let ko = tags.contains(&1);
        Decision {
            language: match (en, ko) {
                (true, true) => Language::Mixed,
                (_, true) => Language::Korean,
                _ if self.on[0] => Language::English,
                _ => Language::Korean,
            },
            output: if keys.is_ascii() {
                render(keys, &tags)
            } else {
                keys.to_string()
            },
            confidence,
            explanation: "learned key n-gram CRF".into(),
            runner_up: None,
        }
    }
}

fn sample(text: &str, previous: Option<Language>) -> Option<Sample> {
    let mut keys = String::new();
    let mut labels = Vec::new();
    for c in text.chars() {
        let (typed, tag) = if c.is_ascii_alphabetic() {
            (c.to_string(), 0)
        } else if ('가'..='힣').contains(&c) {
            (crate::keys_of(&c.to_string()), 1)
        } else if ('ㄱ'..='ㅣ').contains(&c) {
            let k = ('A'..='z').find(|k| DubeolsikLayout::jamo(*k) == Some(c))?;
            (k.to_string(), 1)
        } else {
            return None;
        };
        labels.extend(std::iter::repeat_n(tag, typed.len()));
        keys.push_str(&typed);
    }
    if keys.is_empty() || render(&keys, &labels) != text {
        return None;
    }
    let features = features(keys.as_bytes(), previous);
    Some(Sample {
        keys,
        expected: text.into(),
        labels,
        previous,
        features,
    })
}

fn mixed_text(a: &Sample, b: &Sample, index: usize, augment: bool) -> String {
    if !augment {
        return a.expected.clone() + &b.expected;
    }
    if a.labels.iter().all(|t| *t == 1) {
        let cut = a
            .expected
            .char_indices()
            .nth(index % a.expected.chars().count() + 1)
            .map_or(a.expected.len(), |(i, _)| i);
        a.expected[..cut].to_string() + &b.expected
    } else {
        let cut = b
            .expected
            .char_indices()
            .nth(index % b.expected.chars().count())
            .unwrap()
            .0;
        a.expected.clone() + &b.expected[cut..]
    }
}

fn training_segments(s: &Sample) -> bool {
    let mut start = 0;
    while start < s.labels.len() {
        let mut end = start + 1;
        while end < s.labels.len() && s.labels[end] == s.labels[start] {
            end += 1;
        }
        if crate::tune::group_fold(&s.keys[start..end]) == 0 {
            return false;
        }
        start = end;
    }
    true
}

pub fn fit(path: &Path, augment: bool) -> Result<(), String> {
    let mut train = Vec::new();
    let mut held = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in ["en_sentences.txt", "ko_sentences.txt", "ko_comments.txt"] {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("data")
                .join(name),
        )
        .map_err(|e| e.to_string())?;
        let mut items = Vec::new();
        for line in text.lines().skip(1).step_by(3) {
            let mut previous = None;
            for word in line.split(|c: char| {
                !(c.is_ascii_alphabetic()
                    || ('가'..='힣').contains(&c)
                    || ('ㄱ'..='ㅣ').contains(&c))
            }) {
                if let Some(s) = sample(word, previous) {
                    previous = Some(if *s.labels.last().unwrap() == 1 {
                        Language::Korean
                    } else {
                        Language::English
                    });
                    if seen.insert((s.keys.clone(), s.expected.clone(), s.previous)) {
                        items.push(s);
                    }
                }
            }
        }
        let step = (items.len() / 4000).max(1);
        for s in items.into_iter().step_by(step).take(4000) {
            let group = s
                .expected
                .split(|c: char| !c.is_ascii_alphabetic())
                .find(|w| !w.is_empty())
                .unwrap_or(&s.keys);
            if crate::tune::group_fold(group) == 0 {
                held.push(s);
            } else {
                train.push(s);
            }
        }
    }
    // Mixed examples come from training words; no hand-written particles or evaluation cases.
    let en: Vec<_> = train
        .iter()
        .filter(|s| s.labels.iter().all(|t| *t == 0))
        .collect();
    let ko: Vec<_> = train
        .iter()
        .filter(|s| s.labels.iter().all(|t| *t == 1))
        .collect();
    if en.is_empty() || ko.is_empty() {
        return Err("both training languages are required".into());
    }
    let mut mixed = Vec::new();
    for i in 0..16000 {
        let (a, b) = if i % 2 == 0 {
            (en[i % en.len()], ko[(i * 131) % ko.len()])
        } else {
            (ko[i % ko.len()], en[(i * 131) % en.len()])
        };
        if let Some(s) = sample(&mixed_text(a, b, i, augment), a.previous) {
            // A cut fragment also needs its own training fold, not just its parent word.
            if training_segments(&s) {
                mixed.push(s);
            }
        }
        if mixed.len() == 4000 {
            break;
        }
    }
    train.extend(mixed);
    println!(
        "CRF: train={} held-out={} parameters={SIZE} augment-spans={augment}",
        train.len(),
        held.len()
    );
    let mut model = Model {
        weights: vec![0.0; SIZE],
    };
    let mut accumulated = vec![0.0; SIZE];
    let mut order: Vec<_> = (0..train.len()).collect();
    let mut rng = 1u64;
    for epoch in 0..6 {
        for i in (1..order.len()).rev() {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            order.swap(i, (rng as usize) % (i + 1));
        }
        let mut loss = 0.0;
        for &i in &order {
            loss += model.train(&train[i], &mut accumulated);
        }
        println!(
            "epoch {}: mean training NLL {:.5}",
            epoch + 1,
            loss / train.len() as f64
        );
    }
    let baseline = tacit_core::make_engine(None, &tacit_core::Languages::legacy(false));
    let engine = Engine {
        model: Arc::new(model),
        on: [true, true],
    };
    for (name, e) in [
        ("baseline", baseline.as_ref() as &dyn DecisionEngine),
        ("CRF", &engine as &dyn DecisionEngine),
    ] {
        let hits = held
            .iter()
            .filter(|s| {
                e.decide(
                    &s.keys,
                    &DecisionContext {
                        previous_language: s.previous,
                        ..Default::default()
                    },
                )
                .output
                    == s.expected
            })
            .count();
        println!(
            "independent word groups {name}: {hits}/{} ({:.3}%)",
            held.len(),
            100.0 * hits as f64 / held.len().max(1) as f64
        );
    }
    let mut bytes = MAGIC.to_vec();
    for w in &engine.model.weights {
        bytes.extend(w.to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn span_augmentation_uses_unicode_boundaries_and_remains_typeable() {
        let en = sample("hello", None).unwrap();
        let ko = sample("한국어", None).unwrap();
        assert_eq!(mixed_text(&en, &ko, 1, true), "hello국어");
        assert_eq!(mixed_text(&ko, &en, 1, true), "한국hello");
        for i in 0..9 {
            for (a, b) in [(&en, &ko), (&ko, &en)] {
                assert!(sample(&mixed_text(a, b, i, true), None).is_some());
            }
        }
        let held = (b'a'..=b'z')
            .map(|c| format!("word{}", c as char))
            .find(|w| crate::tune::group_fold(w) == 0)
            .unwrap();
        assert!(!training_segments(
            &sample(&(held + "한국어"), None).unwrap()
        ));
    }
    #[test]
    fn partition_equals_enumeration_and_training_reduces_loss() {
        let mut model = Model {
            weights: (0..SIZE).map(|i| (i % 7) as f64 / 10.0).collect(),
        };
        let s = sample("안녕", None).unwrap();
        let e = model.emissions(s.keys.as_bytes(), &s.features, [true, true]);
        let (_, z) = model.forward(&e);
        let mut sum = 0.0;
        for bits in 0..(1usize << s.keys.len()) {
            let tags: Vec<_> = (0..s.keys.len()).map(|i| (bits >> i) & 1).collect();
            sum += model.score(&e, &tags).exp();
        }
        assert!((sum.ln() - z).abs() < 1e-9);
        let before = z - model.score(&e, &s.labels);
        model.train(&s, &mut vec![0.0; SIZE]);
        let e = model.emissions(s.keys.as_bytes(), &s.features, [true, true]);
        assert!(model.forward(&e).1 - model.score(&e, &s.labels) < before);
        let (tags, p) = model.decode(&s.keys, None, [true, true]).unwrap();
        assert_eq!(render(&s.keys, &tags), s.expected);
        assert!((0.0..=1.0).contains(&p));
    }
}
