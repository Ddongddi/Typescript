//! Scenario sets by usage type (chat, dev, writing): small hand-written sentences in
//! lab/scenarios/*.tsv, each tagged with a scenario. They are committed (a few KB, no data
//! to fetch), so every run measures them, and the report breaks accuracy down per type and
//! per scenario. Two conditions: each sentence alone, and four sentences of one type chained
//! into one conversion (the context carried from one word to the next).

use std::collections::BTreeMap;
use std::time::Instant;

use typescript_core::{DecisionEngine, TextConverter};

use crate::corpus::{token_hits, Report};
use crate::keys_of_text;

const TYPES: [(&str, &str); 3] = [
    ("chat", include_str!("../scenarios/chat.tsv")),
    ("dev", include_str!("../scenarios/dev.tsv")),
    ("writing", include_str!("../scenarios/writing.tsv")),
];

/// Sentences chained into one conversion in the second condition.
const CHAIN: usize = 4;

#[derive(Default)]
pub(crate) struct Tally {
    sentences: usize,
    exact: usize,
    tokens: usize,
    token_hits: usize,
}

impl Tally {
    pub(crate) fn add(&mut self, expected: &str, got: &str) {
        let (hits, n) = token_hits(expected, got);
        self.sentences += 1;
        self.exact += usize::from(expected == got);
        self.tokens += n;
        self.token_hits += hits;
    }

    pub(crate) fn merge(&mut self, other: &Tally) {
        self.sentences += other.sentences;
        self.exact += other.exact;
        self.tokens += other.tokens;
        self.token_hits += other.token_hits;
    }

    pub(crate) fn sentence_pct(&self) -> f64 {
        100.0 * self.exact as f64 / self.sentences.max(1) as f64
    }

    fn token_pct(&self) -> f64 {
        100.0 * self.token_hits as f64 / self.tokens.max(1) as f64
    }
}

fn items(text: &str) -> Vec<(&str, &str)> {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            l.split_once('\t')
                .unwrap_or_else(|| panic!("scenario line without a tab: {l:?}"))
        })
        .collect()
}

fn push(report: &mut Report, name: String, value: f64) {
    report.metrics.push((name, value));
}

/// One type: each sentence alone (per scenario and overall), then chained, plus the speed.
fn run_type(
    engine: &dyn DecisionEngine,
    name: &str,
    text: &str,
    shown: usize,
    report: &mut Report,
) -> Tally {
    let converter = TextConverter::new(engine);
    let items = items(text);
    let mut scenarios: BTreeMap<&str, Tally> = BTreeMap::new();
    let mut misses = Vec::new();
    let (mut chars, mut elapsed) = (0usize, 0f64);
    for (expected, scenario) in &items {
        let keys = keys_of_text(expected);
        let start = Instant::now();
        let got = converter.convert(&keys).0;
        elapsed += start.elapsed().as_secs_f64() * 1e6;
        chars += keys.chars().count();
        scenarios.entry(scenario).or_default().add(expected, &got);
        if got != *expected {
            misses.push(format!("[{scenario}] {keys} → {got} (want {expected})"));
        }
    }
    let mut all = Tally::default();
    for (scenario, tally) in &scenarios {
        push(
            report,
            format!("sc.{name}.{scenario}"),
            tally.sentence_pct(),
        );
        all.merge(tally);
    }
    push(report, format!("sc.{name}.sentence"), all.sentence_pct());
    push(report, format!("sc.{name}.token"), all.token_pct());
    push(
        report,
        format!("sc.{name}.us_per_char"),
        elapsed / chars as f64,
    );

    let mut chained = Tally::default();
    for group in items.chunks(CHAIN).filter(|g| g.len() == CHAIN) {
        let expected = group.iter().map(|(e, _)| *e).collect::<Vec<_>>().join(" ");
        let got = converter.convert(&keys_of_text(&expected)).0;
        chained.add(&expected, &got);
        if got != expected {
            misses.push(format!("[chain] {expected} → {got}"));
        }
    }
    push(
        report,
        format!("sc.{name}.chain.token"),
        chained.token_pct(),
    );
    misses.truncate(shown.max(1) * 4);
    report.misses.push((format!("sc.{name}"), misses));
    all
}

pub fn run(engine: &dyn DecisionEngine, shown: usize) -> Report {
    let mut report = Report {
        metrics: Vec::new(),
        misses: Vec::new(),
    };
    let mut all = Tally::default();
    for (name, text) in TYPES {
        all.merge(&run_type(engine, name, text, shown, &mut report));
    }
    push(&mut report, "sc.ALL.sentence".into(), all.sentence_pct());
    push(&mut report, "sc.ALL.token".into(), all.token_pct());
    report
}
