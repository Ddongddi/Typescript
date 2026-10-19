//! Lab-only conditional candidate ranker. No evaluation cases or token identities are features.

use std::{collections::HashSet, path::Path, sync::Arc};
use tacit_core::decide::codeswitch::{CodeSwitch, Params};
use tacit_core::lang::korean::KoreanModel;
use tacit_core::{Decision, DecisionContext, DecisionEngine, EnglishModel, Language, Languages};

const FEATURES: usize = 24;
const SIZE: usize = 3 * FEATURES;
const MAGIC: &[u8; 8] = b"TSRANK01";
const DUAL_MAGIC: &[u8; 8] = b"TSRANK02";

struct Candidate {
    output: String,
    language: Language,
    features: [f64; FEATURES],
    partial: bool,
}

struct Generator {
    baseline: Arc<tacit_core::LearningEngine>,
    mixed: CodeSwitch,
    korean: KoreanModel,
}

impl Generator {
    fn new() -> Self {
        Self {
            baseline: tacit_core::make_engine(None, &Languages::legacy(false)),
            mixed: CodeSwitch::new(EnglishModel::shared(), Params::default()).without_skip(),
            korean: KoreanModel::default(),
        }
    }

    fn candidates(&self, keys: &str, context: &DecisionContext) -> Vec<Candidate> {
        let baseline = self.baseline.decide(keys, context);
        let english = EnglishModel::shared();
        let korean = self.korean.score(keys, context.partial);
        let hangul = korean.as_ref().map(|s| s.composition.text());
        // shortcut: one mixed path, offer N-best if measured candidate coverage is limiting.
        let split = self.mixed.analyze(keys, context);
        let split_text = split.as_ref().map(|s| s.text());
        let n = keys.len().max(1) as f64;
        let mut readings = vec![(baseline.output.clone(), baseline.language, None)];
        readings.push((keys.to_string(), Language::English, None));
        if let Some(text) = &hangul {
            readings.push((text.clone(), Language::Korean, None));
        }
        if let Some(split) = &split {
            readings.push((split.text(), Language::Mixed, Some(split)));
        }
        let mut seen = HashSet::new();
        readings
            .into_iter()
            .filter_map(|(output, language, candidate_split)| {
                if !seen.insert(output.clone()) {
                    return None;
                }
                let matches_base = output == baseline.output;
                let split = candidate_split.or_else(|| {
                    split
                        .as_ref()
                        .filter(|_| split_text.as_ref() == Some(&output))
                });
                let f = [
                    1.0,
                    n.ln_1p(),
                    english.average_log_probability(keys),
                    korean.as_ref().map_or(0.0, |s| s.average_log_probability),
                    english.word_log_probability(keys).unwrap_or(0.0) / n,
                    hangul
                        .as_ref()
                        .and_then(|s| self.korean.word_log_probability(s))
                        .unwrap_or(0.0)
                        / n,
                    english.level(keys).map_or(0.0, |l| f64::from(l) / 100.0),
                    f64::from(english.is_known_word(keys)),
                    f64::from(
                        hangul
                            .as_ref()
                            .is_some_and(|s| self.korean.is_known_word_or_inflected(s)),
                    ),
                    korean
                        .as_ref()
                        .map_or(0.0, |s| s.composition.stray_jamo().len() as f64 / n),
                    keys.bytes().filter(u8::is_ascii_uppercase).count() as f64 / n,
                    f64::from(context.previous_language == Some(Language::English)),
                    f64::from(
                        context.previous_language.map(Language::sentence_language)
                            == Some(Language::Korean),
                    ),
                    f64::from(context.after_digit),
                    f64::from(context.before_digit),
                    f64::from(context.partial),
                    f64::from(matches_base),
                    if matches_base {
                        baseline.confidence
                    } else {
                        0.0
                    },
                    split.map_or(0.0, |s| s.score / n),
                    split
                        .filter(|s| s.gap.is_finite())
                        .map_or(0.0, |s| s.gap / n),
                    split.map_or(0.0, |s| s.pieces.len().saturating_sub(1) as f64 / n),
                    output.chars().filter(char::is_ascii_alphabetic).count() as f64 / n,
                    output.chars().filter(|c| ('가'..='힣').contains(c)).count() as f64 / n,
                    f64::from(split.is_some()),
                ];
                assert!(
                    f.iter().all(|x| x.is_finite()),
                    "non-finite candidate features: {keys}"
                );
                Some(Candidate {
                    output,
                    language,
                    features: f,
                    partial: context.partial,
                })
            })
            .collect()
    }
}

fn class(language: Language) -> usize {
    match language {
        Language::English => 0,
        Language::Korean => 1,
        _ => 2,
    }
}

pub struct Model {
    weights: Vec<f64>,
}

impl Model {
    fn offset(&self, candidate: &Candidate) -> usize {
        class(candidate.language) * FEATURES
            + if self.weights.len() == 2 * SIZE && candidate.partial {
                SIZE
            } else {
                0
            }
    }
    fn probabilities(&self, candidates: &[Candidate]) -> Vec<f64> {
        let mut scores: Vec<_> = candidates
            .iter()
            .map(|c| {
                self.weights[self.offset(c)..][..FEATURES]
                    .iter()
                    .zip(c.features)
                    .map(|(w, f)| w * f)
                    .sum::<f64>()
            })
            .collect();
        assert!(
            scores.iter().all(|s| s.is_finite()),
            "non-finite ranker score"
        );
        let max = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        for score in &mut scores {
            *score = (*score - max).exp();
        }
        let z: f64 = scores.iter().sum();
        for score in &mut scores {
            *score /= z;
        }
        scores
    }

    fn train(&mut self, candidates: &[Candidate], gold: usize, accumulated: &mut [f64]) -> f64 {
        let probabilities = self.probabilities(candidates);
        let loss = -probabilities[gold].max(f64::MIN_POSITIVE).ln();
        let mut gradient = vec![0.0; self.weights.len()];
        for (i, candidate) in candidates.iter().enumerate() {
            let delta = f64::from(i == gold) - probabilities[i];
            for (j, feature) in candidate.features.iter().enumerate() {
                gradient[self.offset(candidate) + j] += delta * feature;
            }
        }
        for (i, gradient) in gradient.into_iter().enumerate() {
            accumulated[i] += gradient * gradient;
            self.weights[i] += 0.05 * gradient / (accumulated[i] + 1e-8).sqrt();
        }
        loss
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        if size != (8 + SIZE * 8) as u64 && size != (8 + 2 * SIZE * 8) as u64 {
            return Err("invalid ranker size".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if !((bytes.len() == 8 + SIZE * 8 && bytes.get(..8) == Some(MAGIC))
            || (bytes.len() == 8 + 2 * SIZE * 8 && bytes.get(..8) == Some(DUAL_MAGIC)))
        {
            return Err("invalid ranker header/size".into());
        }
        let weights: Vec<_> = bytes[8..]
            .chunks_exact(8)
            .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
            .collect();
        if !weights.iter().all(|w| w.is_finite()) {
            return Err("non-finite ranker weight".into());
        }
        Ok(Self { weights })
    }
}

pub struct Engine {
    model: Arc<Model>,
    generator: Generator,
    preview_only: bool,
}

impl Engine {
    pub fn new(model: Arc<Model>, preview_only: bool) -> Self {
        Self {
            model,
            generator: Generator::new(),
            preview_only,
        }
    }
}

impl DecisionEngine for Engine {
    fn decide(&self, keys: &str, context: &DecisionContext) -> Decision {
        if (self.preview_only && !context.partial)
            || keys.is_empty()
            || !keys.bytes().all(|b| b.is_ascii_alphabetic())
        {
            return self.generator.baseline.decide(keys, context);
        }
        let candidates = self.generator.candidates(keys, context);
        let probabilities = self.model.probabilities(&candidates);
        let mut order: Vec<_> = (0..candidates.len()).collect();
        order.sort_by(|a, b| probabilities[*b].total_cmp(&probabilities[*a]));
        let best = order[0];
        Decision {
            language: candidates[best].language,
            output: candidates[best].output.clone(),
            // Conditional on the returned candidates; not a calibrated correctness probability.
            confidence: probabilities[best],
            runner_up: order.get(1).map(|i| candidates[*i].language),
            explanation: "learned common candidate ranker".into(),
        }
    }
}

fn partition(sample: &super::learned::Sample, held: bool) -> bool {
    partition_keys(&sample.keys, &sample.labels, held)
}

fn partition_keys(keys: &str, labels: &[usize], held: bool) -> bool {
    if (super::tune::group_fold(keys) == 0) != held {
        return false;
    }
    let mut start = 0;
    while start < keys.len() {
        let mut end = start + 1;
        while end < keys.len() && labels[end] == labels[start] {
            end += 1;
        }
        if (super::tune::group_fold(&keys[start..end]) == 0) != held {
            return false;
        }
        start = end;
    }
    true
}

fn samples(
    extra: Option<&Path>,
) -> Result<(Vec<super::learned::Sample>, Vec<super::learned::Sample>), String> {
    let mut train = Vec::new();
    let mut held = Vec::new();
    let mut seen = HashSet::new();
    let normalize = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let mut texts: Vec<_> = ["en_sentences.txt", "ko_sentences.txt", "ko_comments.txt"]
        .into_iter()
        .map(|name| {
            std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("data")
                    .join(name),
            )
            .map(|text| (text, false))
            .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    let forbidden: HashSet<_> = texts
        .iter()
        .flat_map(|(text, _)| text.lines().enumerate())
        .filter(|(i, _)| i % 3 != 1)
        .map(|(_, s)| normalize(s))
        .collect();
    if let Some(path) = extra {
        texts.push((
            std::fs::read_to_string(path).map_err(|e| e.to_string())?,
            true,
        ));
    }
    for (text, all_rows) in &texts {
        let mut items = Vec::new();
        for line in text
            .lines()
            .enumerate()
            .filter(|(i, _)| *all_rows || i % 3 == 1)
            .map(|(_, line)| line)
            .filter(|line| !forbidden.contains(&normalize(line)))
        {
            let mut previous = None;
            for word in line.split(|c: char| {
                !(c.is_ascii_alphabetic()
                    || ('가'..='힣').contains(&c)
                    || ('ㄱ'..='ㅣ').contains(&c))
            }) {
                if let Some(s) = super::learned::sample(word, previous) {
                    previous = Some(if *s.labels.last().unwrap() == 0 {
                        Language::English
                    } else {
                        Language::Korean
                    });
                    if seen.insert((s.keys.clone(), s.expected.clone(), s.previous)) {
                        items.push(s);
                    }
                }
            }
        }
        let step = (items.len() / 4000).max(1);
        for s in items.into_iter().step_by(step).take(4000) {
            if partition(&s, false) {
                train.push(s);
            } else if partition(&s, true) {
                held.push(s);
            }
        }
    }
    println!(
        "natural training tokens={} English-after-Korean={} mixed={}",
        train.len(),
        train
            .iter()
            .filter(|s| s.labels.iter().all(|t| *t == 0) && s.previous == Some(Language::Korean))
            .count(),
        train
            .iter()
            .filter(|s| s.labels.contains(&0) && s.labels.contains(&1))
            .count()
    );
    for (items, held_out) in [(&mut train, false), (&mut held, true)] {
        let en: Vec<_> = items
            .iter()
            .filter(|s| s.labels.iter().all(|t| *t == 0))
            .collect();
        let ko: Vec<_> = items
            .iter()
            .filter(|s| s.labels.iter().all(|t| *t == 1))
            .collect();
        if en.is_empty() || ko.is_empty() {
            return Err("both training languages are required".into());
        }
        let mut mixed = Vec::new();
        let mut unique = HashSet::new();
        for i in 0..80_000 {
            let (a, b) = if i % 2 == 0 {
                (en[i % en.len()], ko[(i * 131) % ko.len()])
            } else {
                (ko[i % ko.len()], en[(i * 131) % en.len()])
            };
            if let Some(s) =
                super::learned::sample(&super::learned::mixed_text(a, b, i, false), a.previous)
            {
                if partition(&s, held_out) && unique.insert(s.keys.clone()) {
                    mixed.push(s);
                }
            }
            if mixed.len() == if held_out { 1000 } else { 4000 } {
                break;
            }
        }
        items.extend(mixed);
    }
    let train_keys: HashSet<_> = train.iter().map(|s| s.keys.to_lowercase()).collect();
    assert!(held
        .iter()
        .all(|s| !train_keys.contains(&s.keys.to_lowercase())));
    Ok((train, held))
}

struct Example {
    candidates: Vec<Candidate>,
    gold: Option<usize>,
    partial: bool,
}

fn examples(
    samples: &[super::learned::Sample],
    generator: &Generator,
    prefixes: bool,
    augment_context: bool,
) -> Vec<Example> {
    let mut examples = Vec::new();
    for sample in samples {
        let mut lengths = vec![(sample.keys.len(), false)];
        if prefixes {
            // Replay visits every prefix, including full spelling before its delimiter.
            lengths.extend((1..=sample.keys.len()).map(|end| (end, true)));
        }
        for (end, partial) in lengths {
            let held = super::tune::group_fold(&sample.keys) == 0;
            if !partition_keys(&sample.keys[..end], &sample.labels[..end], held) {
                continue;
            }
            let expected = if partial {
                super::learned::render(&sample.keys[..end], &sample.labels[..end])
            } else {
                sample.expected.clone()
            };
            let contexts = if augment_context {
                vec![None, Some(Language::English), Some(Language::Korean)]
            } else {
                vec![sample.previous]
            };
            for previous in contexts {
                let candidates = generator.candidates(
                    &sample.keys[..end],
                    &DecisionContext {
                        previous_language: previous,
                        partial,
                        ..Default::default()
                    },
                );
                let gold = candidates.iter().position(|c| c.output == expected);
                examples.push(Example {
                    candidates,
                    gold,
                    partial,
                });
            }
        }
    }
    examples
}

fn report(name: &str, examples: &[Example], model: &Model) {
    let mut covered = 0;
    let mut hits = 0;
    let mut baseline_hits = 0;
    for e in examples {
        covered += usize::from(e.gold.is_some());
        baseline_hits += usize::from(e.gold == Some(0));
        let p = model.probabilities(&e.candidates);
        let best = (0..p.len())
            .max_by(|a, b| p[*a].total_cmp(&p[*b]).then_with(|| b.cmp(a)))
            .unwrap();
        hits += usize::from(e.gold == Some(best));
    }
    println!(
        "{name}: hits={hits}/{} coverage={covered}/{} baseline={baseline_hits}",
        examples.len(),
        examples.len()
    );
}

pub fn fit(
    path: &Path,
    prefixes: bool,
    augment_context: bool,
    separate_heads: bool,
    extra: Option<&Path>,
) -> Result<(), String> {
    assert!(
        !separate_heads || prefixes,
        "separate heads require prefix training"
    );
    let (train, held) = samples(extra)?;
    let size = if separate_heads { 2 * SIZE } else { SIZE };
    println!("ranker: train={} held={} parameters={size} prefix-training={prefixes} context-augmentation={augment_context} separate-heads={separate_heads}; overlapping key groups=0", train.len(), held.len());
    let generator = Generator::new();
    let train = examples(&train, &generator, prefixes, augment_context);
    let held = examples(&held, &generator, true, false);
    let coverage = train.iter().filter(|e| e.gold.is_some()).count();
    println!(
        "training candidate coverage={coverage}/{} (missing gold is reported, not injected)",
        train.len()
    );
    let mut model = Model {
        weights: vec![0.0; size],
    };
    let mut accumulated = vec![0.0; size];
    let mut order: Vec<_> = (0..train.len()).collect();
    let mut rng = 1u64;
    for epoch in 0..6 {
        for i in (1..order.len()).rev() {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            order.swap(i, rng as usize % (i + 1));
        }
        let mut loss = 0.0;
        for &i in &order {
            if let Some(gold) = train[i].gold {
                loss += model.train(&train[i].candidates, gold, &mut accumulated);
            }
        }
        println!(
            "epoch {}: mean NLL {:.5}",
            epoch + 1,
            loss / coverage.max(1) as f64
        );
    }
    report("train", &train, &model);
    report("held-out key groups including prefixes", &held, &model);
    let full: Vec<_> = held.into_iter().filter(|e| !e.partial).collect();
    report("held-out complete keys", &full, &model);
    let mut bytes = if separate_heads { DUAL_MAGIC } else { MAGIC }.to_vec();
    for w in model.weights {
        bytes.extend(w.to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fully_spelled_token_is_trained_before_and_after_its_delimiter() {
        let sample = super::super::learned::sample("hello", None).unwrap();
        let generator = Generator::new();
        let examples = examples(std::slice::from_ref(&sample), &generator, true, false);
        for partial in [false, true] {
            let example = examples
                .iter()
                .find(|e| e.partial == partial && e.candidates[0].output == sample.keys)
                .expect("full spelling must be represented in both session states");
            assert_eq!(
                example.candidates[example.gold.unwrap()].output,
                sample.expected
            );
        }
    }

    #[test]
    fn training_one_head_leaves_the_other_head_unchanged() {
        let generator = Generator::new();
        let s = super::super::learned::sample("안녕", None).unwrap();
        let mut model = Model {
            weights: vec![0.0; 2 * SIZE],
        };
        let mut accumulated = vec![0.0; 2 * SIZE];
        for partial in [false, true] {
            let candidates = generator.candidates(
                &s.keys,
                &DecisionContext {
                    partial,
                    ..Default::default()
                },
            );
            let gold = candidates
                .iter()
                .position(|c| c.output == s.expected)
                .unwrap();
            let before = model.weights.clone();
            model.train(&candidates, gold, &mut accumulated);
            let untouched = if partial { 0..SIZE } else { SIZE..2 * SIZE };
            assert_eq!(model.weights[untouched.clone()], before[untouched]);
            assert_ne!(model.weights, before);
        }
        let path =
            std::env::temp_dir().join(format!("tacit-ranker-dual-{}.bin", std::process::id()));
        let mut bytes = DUAL_MAGIC.to_vec();
        for weight in &model.weights {
            bytes.extend(weight.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(Model::load(&path).unwrap().weights, model.weights);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn preview_only_uses_the_original_decision_when_a_token_closes() {
        let engine = Engine::new(
            Arc::new(Model {
                weights: vec![0.0; SIZE],
            }),
            true,
        );
        for keys in ["hello", "dkssud", "githubdp"] {
            let context = DecisionContext::default();
            let expected = engine.generator.baseline.decide(keys, &context);
            let actual = engine.decide(keys, &context);
            assert_eq!(actual.output, expected.output);
            assert_eq!(actual.language, expected.language);
            assert_eq!(actual.confidence.to_bits(), expected.confidence.to_bits());
        }
    }

    #[test]
    fn a_held_out_component_cannot_enter_training_inside_a_new_mixed_token() {
        let held = ('a'..='z')
            .map(|c| format!("word{c}"))
            .find(|s| super::super::tune::group_fold(s) == 0)
            .unwrap();
        let mixed = ('가'..='힣')
            .filter_map(|c| super::super::learned::sample(&format!("{held}{c}"), None))
            .find(|s| super::super::tune::group_fold(&s.keys) != 0)
            .unwrap();
        assert!(!partition(&mixed, false));
    }

    #[test]
    fn candidate_softmax_gradient_and_binary_validation() {
        let generator = Generator::new();
        let s = super::super::learned::sample("안녕", None).unwrap();
        let candidates = generator.candidates(&s.keys, &DecisionContext::default());
        let gold = candidates
            .iter()
            .position(|c| c.output == s.expected)
            .unwrap();
        assert!(candidates.iter().any(|c| c.output == s.keys));
        let mut model = Model {
            weights: vec![0.0; SIZE],
        };
        let p = model.probabilities(&candidates);
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        let before = -p[gold].ln();
        model.train(&candidates, gold, &mut vec![0.0; SIZE]);
        assert!(-model.probabilities(&candidates)[gold].ln() < before);
        let key = &s.keys;
        for end in 1..key.len() {
            let c = generator.candidates(
                &key[..end],
                &DecisionContext {
                    partial: true,
                    ..Default::default()
                },
            );
            assert!(c.iter().all(|c| c.features.iter().all(|f| f.is_finite())));
            assert_eq!(
                c.len(),
                c.iter().map(|c| &c.output).collect::<HashSet<_>>().len()
            );
        }
        let path =
            std::env::temp_dir().join(format!("tacit-ranker-invalid-{}.bin", std::process::id()));
        std::fs::write(&path, MAGIC).unwrap();
        assert!(Model::load(&path).is_err());
        let mut bytes = MAGIC.to_vec();
        bytes.extend(std::iter::repeat_n(f64::NAN.to_le_bytes(), SIZE).flatten());
        std::fs::write(&path, bytes).unwrap();
        assert!(Model::load(&path).is_err());
        let mut bytes = MAGIC.to_vec();
        for weight in &model.weights {
            bytes.extend(weight.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(Model::load(&path).unwrap().weights, model.weights);
        std::fs::remove_file(path).unwrap();
    }
}
