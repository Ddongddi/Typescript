//! Actual InputSession key replay, without completion or future-text correction.

use std::{collections::BTreeMap, sync::atomic::Ordering::Relaxed, sync::Arc, time::Instant};
use tacit_core::session::testing::FakeTextField;
use tacit_core::{AppPolicies, InputSession, Key, Language, LearningEngine, TextClient};

fn language(text: &str) -> Language {
    let english = text.chars().any(|c| c.is_ascii_alphabetic());
    let korean = text
        .chars()
        .any(|c| ('ㄱ'..='ㅣ').contains(&c) || ('가'..='힣').contains(&c));
    match (english, korean) {
        (true, true) => Language::Mixed,
        (_, true) => Language::Korean,
        _ => Language::English,
    }
}

fn replay(
    engine: Arc<LearningEngine>,
    lines: &[&str],
    diagnostic_limit: usize,
) -> BTreeMap<String, f64> {
    let mut wrong = 0;
    let mut changes = 0;
    let mut exact = 0;
    let mut mismatch = 0;
    let mut literal_jamo = 0;
    let mut times = Vec::new();
    let (mut allocations, mut allocated_bytes) = (0, 0);
    let mut causes = BTreeMap::<&str, usize>::new();
    let mut previews = Vec::new();
    let mut commits = Vec::new();
    for line in lines {
        let mut session = InputSession::new(engine.clone(), AppPolicies::default(), true);
        let mut client = FakeTextField::new();
        let mut committed = String::new();
        let mut keys = String::new();
        let mut labels = Vec::new();
        let mut shown = None;
        for c in line.chars() {
            if let Some(sample) = super::learned::sample(&c.to_string(), None) {
                for (key, tag) in sample.keys.chars().zip(sample.labels) {
                    keys.push(key);
                    labels.push(tag);
                    let before = (
                        super::ALLOCATIONS.load(Relaxed),
                        super::ALLOCATED_BYTES.load(Relaxed),
                    );
                    let start = Instant::now();
                    session.handle(Key::Letter(key), &mut client);
                    let spent = start.elapsed();
                    allocations += super::ALLOCATIONS.load(Relaxed) - before.0;
                    allocated_bytes += super::ALLOCATED_BYTES.load(Relaxed) - before.1;
                    times.push(spent);
                    let expected = super::learned::render(&keys, &labels);
                    let text = client.text();
                    let current_word = text.strip_prefix(&committed).unwrap_or(&text);
                    if current_word != expected {
                        wrong += 1;
                        let trace = session.last_trace();
                        let cause = match trace {
                            Some(t) if t.keys != keys => "split_or_stale_trace",
                            Some(t) if t.decision.output == expected => "presentation",
                            Some(_) => "decision",
                            None => "no_trace",
                        };
                        *causes.entry(cause).or_default() += 1;
                        if previews.len() < diagnostic_limit {
                            previews.push(format!("  preview {cause}: keys={keys:?} shown={current_word:?} want={expected:?} trace={:?}", trace.map(|t| (&t.keys, t.previous, t.decision.language, t.decision.confidence, &t.decision.output))));
                        }
                    }
                    let current = language(current_word);
                    changes += usize::from(shown.is_some_and(|previous| previous != current));
                    shown = Some(current);
                }
            } else {
                literal_jamo += usize::from(('ㄱ'..='ㅣ').contains(&c));
                if !session.handle(Key::Printable(c), &mut client) {
                    client.insert_text(&c.to_string(), None);
                }
                committed = client.text();
                keys.clear();
                labels.clear();
                shown = None;
            }
        }
        let before = client.text();
        session.handle(Key::Enter, &mut client);
        let after = client.text();
        mismatch += usize::from(before != after);
        exact += usize::from(after == *line);
        if after != *line && commits.len() < diagnostic_limit {
            commits.push(format!("  commit: shown={after:?} want={line:?}"));
        }
    }
    let mut metrics = BTreeMap::new();
    let count = times.len();
    metrics.insert("switch.keys".into(), count as f64);
    metrics.insert("switch.lines".into(), lines.len() as f64);
    metrics.insert(
        "switch.alloc.calls_per_key".into(),
        allocations as f64 / count.max(1) as f64,
    );
    metrics.insert(
        "switch.alloc.bytes_per_key".into(),
        allocated_bytes as f64 / count.max(1) as f64,
    );
    metrics.insert(
        "switch.wrong_preview_pct".into(),
        100.0 * wrong as f64 / count.max(1) as f64,
    );
    metrics.insert(
        "switch.changes_per_1000_keys".into(),
        1000.0 * changes as f64 / count.max(1) as f64,
    );
    metrics.insert(
        "switch.confirmed_sentence_pct".into(),
        100.0 * exact as f64 / lines.len().max(1) as f64,
    );
    metrics.insert(
        "switch.final_preview_mismatch_pct".into(),
        100.0 * mismatch as f64 / lines.len().max(1) as f64,
    );
    metrics.insert("switch.literal_untypable_jamo".into(), literal_jamo as f64);
    assert_eq!(causes.values().sum::<usize>(), wrong);
    for cause in [
        "decision",
        "presentation",
        "split_or_stale_trace",
        "no_trace",
    ] {
        metrics.insert(
            format!("switch.errors.{cause}"),
            causes.get(cause).copied().unwrap_or(0) as f64,
        );
    }
    if !times.is_empty() {
        super::latency(&mut metrics, "switch.handle", times);
    }
    for detail in previews.into_iter().chain(commits) {
        println!("{detail}");
    }
    metrics
}

pub fn run(
    engine: Arc<LearningEngine>,
    sample: usize,
    fold: Option<(usize, usize)>,
    shown: usize,
    input: Option<std::path::PathBuf>,
) {
    let inputs: Vec<_> = input.map_or_else(
        || {
            ["en_sentences.txt", "ko_sentences.txt", "ko_comments.txt"]
                .into_iter()
                .map(|name| {
                    (
                        name.to_string(),
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("data")
                            .join(name),
                    )
                })
                .collect()
        },
        |path| vec![(path.display().to_string(), path)],
    );
    for (name, path) in inputs {
        let text = std::fs::read_to_string(path).expect("switch replay requires corpus data");
        let lines: Vec<_> = text
            .lines()
            .enumerate()
            .filter(|(i, _)| fold.is_none_or(|(k, n)| i % n == k))
            .map(|(_, line)| line)
            .collect();
        let step = (lines.len() / sample.max(1)).max(1);
        let lines: Vec<_> = lines.into_iter().step_by(step).take(sample).collect();
        println!("{name} (InputSession + FakeTextField; no completion, no look-ahead)");
        for (metric, value) in replay(engine.clone(), &lines, shown) {
            println!("{metric}\t{value:.3}");
        }
    }
    for (name, text) in super::scenario::TYPES {
        let lines: Vec<_> = super::scenario::items(text)
            .into_iter()
            .map(|(line, _)| line)
            .collect();
        println!("scenario.{name} (InputSession + FakeTextField; no completion, no look-ahead)");
        for (metric, value) in replay(engine.clone(), &lines, shown) {
            println!("{metric}\t{value:.3}");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn replay_counts_real_keys_and_commits_without_completion() {
        let engine = tacit_core::make_engine(None, &tacit_core::Languages::legacy(false));
        let metrics = super::replay(engine, &["Hello 안녕하세요", "Hello"], 0);
        assert!(metrics["switch.keys"] > 10.0);
        assert!(metrics["switch.alloc.calls_per_key"] > 0.0);
        assert!(metrics["switch.alloc.bytes_per_key"] > 0.0);
        assert_eq!(metrics["switch.lines"], 2.0);
        assert_eq!(metrics["switch.confirmed_sentence_pct"], 100.0);
        assert_eq!(metrics["switch.final_preview_mismatch_pct"], 0.0);
        let errors: f64 = metrics
            .iter()
            .filter(|(k, _)| k.starts_with("switch.errors."))
            .map(|(_, v)| v)
            .sum();
        assert!(
            (errors * 100.0 / metrics["switch.keys"] - metrics["switch.wrong_preview_pct"]).abs()
                < 1e-10
        );
    }
}
