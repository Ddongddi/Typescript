//! Japanese conversion is measured separately from language detection; no user learning.
use std::collections::BTreeMap;
use std::time::Instant;

use typescript_core::{
    make_engine, DecisionContext, DecisionEngine, DictionaryConverter, JapaneseConverter, Language,
    Languages, RomajiComposer,
};

use crate::corpus::Report;

const CASES: &str = include_str!("../cases/japanese.tsv");

fn rows(text: &str) -> impl Iterator<Item = (&str, &str, &str)> {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let f: Vec<_> = l.split('\t').collect();
            assert_eq!(
                f.len(),
                3,
                "Japanese case needs category, reading, accepted spellings: {l}"
            );
            assert!(
                !f[1].is_empty() && !f[2].is_empty(),
                "empty Japanese case: {l}"
            );
            (f[0], f[1], f[2])
        })
}

pub fn run(shown: usize) -> Report {
    let heap_before = crate::HEAP.load(std::sync::atomic::Ordering::Relaxed);
    let t = Instant::now();
    let converter = DictionaryConverter::new(None);
    let loaded = t.elapsed().as_secs_f64() * 1000.0;
    let loaded_heap = crate::HEAP
        .load(std::sync::atomic::Ordering::Relaxed)
        .saturating_sub(heap_before);
    let engine = make_engine(None, &Languages::legacy(true));
    let mut counts: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut misses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut times = Vec::new();
    for (bucket, kana, expected) in rows(CASES) {
        let t = Instant::now();
        let candidates = converter.candidates(kana);
        times.push(t.elapsed());
        let whole = kana.chars().count();
        let ranked: Vec<_> = candidates
            .iter()
            .filter(|c| c.kana_count == whole)
            .collect();
        let accepted = |text: &str| expected.split('|').any(|e| e == text);
        let first = ranked.first().is_some_and(|c| accepted(&c.text));
        let top5 = ranked.iter().take(5).any(|c| accepted(&c.text));
        for name in [bucket, "ALL"] {
            let entry = counts.entry(format!("ja.convert.{name}")).or_default();
            entry.0 += usize::from(first);
            entry.1 += usize::from(top5);
            entry.2 += 1;
        }
        if !first {
            let m = misses.entry(bucket.to_string()).or_default();
            if m.len() < shown {
                m.push(format!(
                    "{kana} → {} (want {expected})",
                    ranked.first().map_or("", |c| c.text.as_str())
                ));
            }
        }
        if let Some(keys) = RomajiComposer::romaji(kana) {
            for (previous, label) in [(None, "none"), (Some(Language::Japanese), "after-ja")] {
                let d = engine.decide(
                    &keys,
                    &DecisionContext {
                        previous_language: previous,
                        ..Default::default()
                    },
                );
                let entry = counts.entry(format!("ja.detect.{label}")).or_default();
                entry.0 += usize::from(d.language == Language::Japanese);
                entry.2 += 1;
            }
        }
    }
    // English must stay English when Japanese is enabled, including ambiguous romaji words.
    for word in [
        "hello", "world", "google", "github", "sushi", "sake", "name", "time", "data", "make",
        "take", "same", "some", "home", "come", "more", "nice", "true", "false", "return",
    ] {
        let d = engine.decide(word, &DecisionContext::default());
        let entry = counts.entry("ja.guard.english".into()).or_default();
        entry.0 += usize::from(d.language == Language::English);
        entry.2 += 1;
    }
    let mut metrics = BTreeMap::new();
    metrics.insert("ja.load.ms".into(), loaded);
    metrics.insert("ja.heap.loaded_kb".into(), loaded_heap as f64 / 1024.0);
    metrics.insert(
        "ja.dictionary.bytes".into(),
        include_bytes!("../../crates/core/data/ipadic_compact.bin").len() as f64,
    );
    for (name, (first, top5, n)) in counts {
        metrics.insert(format!("{name}.n"), n as f64);
        metrics.insert(format!("{name}.top1"), 100.0 * first as f64 / n as f64);
        if name.starts_with("ja.convert.") {
            metrics.insert(format!("{name}.top5"), 100.0 * top5 as f64 / n as f64);
        }
    }
    crate::latency(&mut metrics, "ja.convert", times);
    Report {
        metrics: metrics.into_iter().collect(),
        misses: misses.into_iter().collect(),
    }
}

pub fn standalone(args: &[String]) {
    // TSV bridge for official external regression regexes, scored by evaluate_japanese.py.
    if let Some(path) = args
        .iter()
        .position(|a| a == "--readings")
        .and_then(|i| args.get(i + 1))
    {
        let text = std::fs::read_to_string(path).expect("read Japanese readings");
        let converter = DictionaryConverter::new(None);
        for kana in text.lines().filter(|l| !l.is_empty()) {
            assert!(!kana.contains('\t'), "reading must not contain tabs");
            let t = Instant::now();
            let candidates = converter.candidates(kana);
            let elapsed = t.elapsed().as_secs_f64() * 1e6;
            let whole = kana.chars().count();
            let texts: Vec<_> = candidates
                .iter()
                .filter(|c| c.kana_count == whole)
                .map(|c| c.text.replace(['\t', '\n', '\r'], " "))
                .collect();
            println!("{kana}\t{elapsed:.3}\t{}", texts.join("\t"));
        }
        return;
    }
    let report = run(10);
    println!("lab {} Japanese", crate::LAB_VERSION);
    for (name, value) in report.metrics {
        println!("{name:<40} {value:.3}");
    }
    for (bucket, misses) in report.misses {
        println!("\n{bucket} misses:");
        for miss in misses {
            println!("  {miss}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_are_unique_and_romaji_round_trips() {
        let mut seen = std::collections::HashSet::new();
        assert!(rows(CASES).count() >= 60);
        for (_, kana, _) in rows(CASES) {
            assert!(seen.insert(kana), "duplicate {kana}");
            if let Some(keys) = RomajiComposer::romaji(kana) {
                assert_eq!(RomajiComposer::compose(&keys, false).text, kana);
            }
        }
    }

    #[test]
    fn japanese_quality_does_not_regress() {
        let report = run(0);
        let metrics: BTreeMap<_, _> = report.metrics.into_iter().collect();
        assert!(metrics["ja.convert.ALL.top1"] >= 80.0);
        assert!(metrics["ja.convert.ALL.top5"] >= 95.0);
        assert_eq!(metrics["ja.guard.english.top1"], 100.0);
        assert_eq!(metrics["ja.detect.after-ja.top1"], 100.0);
    }
}
