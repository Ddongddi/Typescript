//! The big sets: real sentences and words from lab/data (fetched by lab/fetch_data.py, never
//! committed), synthetic code-switching built from them, and a learning simulation.
//!
//! Every Korean item is typed back to QWERTY keys (`keys_of_text`), so the right answer is the
//! item itself. Without lab/data the lab prints a hint and skips all of this.

use std::fs;
use std::path::PathBuf;

use typescript_core::{
    make_engine, DecisionContext, DecisionEngine, Language, Languages, TextConverter,
};

use crate::{keys_of, keys_of_text};

const TERMS: &str = include_str!("../cases/terms.txt");

const PARTICLES: [&str; 28] = [
    "이",
    "가",
    "은",
    "는",
    "을",
    "를",
    "에",
    "에서",
    "로",
    "으로",
    "와",
    "과",
    "도",
    "만",
    "의",
    "에는",
    "에서는",
    "이랑",
    "하고",
    "한테",
    "처럼",
    "까지",
    "부터",
    "라고",
    "이다",
    "했어",
    "해서",
    "했는데",
];
const COUNTERS: [&str; 8] = ["개", "명", "번", "분", "시", "원", "년", "페이지"];

/// Deterministic numbers, so a run means the same on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.next() % items.len()]
    }
}

pub struct Report {
    pub metrics: Vec<(String, f64)>,
    pub misses: Vec<(String, Vec<String>)>,
}

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
}

fn read_lines(name: &str) -> Option<Vec<String>> {
    let text = fs::read_to_string(data_dir().join(name)).ok()?;
    Some(text.lines().map(str::to_string).collect())
}

/// `sample` items spread over the whole list.
fn spread(items: &[String], sample: usize) -> Vec<&String> {
    let step = (items.len() / sample.max(1)).max(1);
    items.iter().step_by(step).take(sample).collect()
}

/// Per-token hits: the tokens at the same place, or none when the token count differs.
fn token_hits(expected: &str, got: &str) -> (usize, usize) {
    let (e, g): (Vec<&str>, Vec<&str>) = (expected.split(' ').collect(), got.split(' ').collect());
    let hits = if e.len() == g.len() {
        e.iter().zip(&g).filter(|(a, b)| a == b).count()
    } else {
        0
    };
    (hits, e.len())
}

#[derive(Default)]
struct Tally {
    sentences: usize,
    exact: usize,
    tokens: usize,
    token_hits: usize,
    misses: Vec<String>,
}

fn sentences(
    engine: &dyn DecisionEngine,
    name: &str,
    items: &[(String, String)],
    shown: usize,
    report: &mut Report,
) {
    let converter = TextConverter::new(engine);
    let mut tally = Tally::default();
    for (keys, expected) in items {
        let got = converter.convert(keys).0;
        let (hits, n) = token_hits(expected, &got);
        tally.sentences += 1;
        tally.tokens += n;
        tally.token_hits += hits;
        if got == *expected {
            tally.exact += 1;
        } else if tally.misses.len() < shown {
            tally
                .misses
                .push(format!("{keys} → {got} (want {expected})"));
        }
    }
    let pct = |a: usize, b: usize| 100.0 * a as f64 / b.max(1) as f64;
    report.metrics.push((
        format!("big.{name}.sentence"),
        pct(tally.exact, tally.sentences),
    ));
    report.metrics.push((
        format!("big.{name}.token"),
        pct(tally.token_hits, tally.tokens),
    ));
    report.misses.push((format!("big.{name}"), tally.misses));
}

fn words(
    engine: &dyn DecisionEngine,
    name: &str,
    titles: &[&String],
    shown: usize,
    report: &mut Report,
) {
    let contexts = [
        (None, "none"),
        (Some(Language::Korean), "after-ko"),
        (Some(Language::English), "after-en"),
    ];
    for (previous, label) in contexts {
        let ctx = DecisionContext {
            previous_language: previous,
            partial: false,
        };
        let (mut ok, mut misses) = (0, Vec::new());
        for word in titles {
            let got = engine.decide(&keys_of(word), &ctx).output;
            if got == **word {
                ok += 1;
            } else if misses.len() < shown {
                misses.push(format!("{} → {got} (want {word})", keys_of(word)));
            }
        }
        report.metrics.push((
            format!("big.{name}.{label}"),
            100.0 * ok as f64 / titles.len().max(1) as f64,
        ));
        report.misses.push((format!("big.{name}/{label}"), misses));
    }
}

/// Korean sentences with an English term (and a particle, a number, or alone) put in.
fn code_switch(korean: &[&String], terms: &[&str], rng: &mut Rng) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for sentence in korean {
        let tokens: Vec<&str> = sentence.split(' ').collect();
        if tokens.len() < 3 {
            continue;
        }
        let at = 1 + rng.next() % (tokens.len() - 1);
        let term = *rng.pick(terms);
        let (kind, inserted) = match rng.next() % 3 {
            0 => ("particle", format!("{term}{}", rng.pick(&PARTICLES))),
            1 => (
                "number",
                format!("{}{}", 1 + rng.next() % 99, rng.pick(&COUNTERS)),
            ),
            _ => ("bare", term.to_string()),
        };
        let mut parts: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        parts.insert(at, inserted);
        out.push((kind, parts.join(" ")));
    }
    out
}

/// One form of a term corrected twice: how many *other* forms does the engine then get right?
fn learning(terms: &[&str], report: &mut Report) {
    let endings = ["이", "을", "에", "에서", "으로", "는", "와", "도"];
    let store = std::env::temp_dir().join(format!("typescript-lab-{}.json", std::process::id()));
    let _ = fs::remove_file(&store);
    let engine = make_engine(Some(store.clone()), &Languages::legacy(false));
    let forms = |term: &str| -> Vec<(String, String)> {
        endings
            .iter()
            .map(|e| (format!("{term}{}", keys_of(e)), format!("{term}{e}")))
            .collect()
    };
    let ctx = DecisionContext::default();
    let right = |engine: &dyn DecisionEngine, set: &[(String, String)]| {
        set.iter()
            .filter(|(k, want)| engine.decide(k, &ctx).output == *want)
            .count()
    };
    let (mut before, mut after, mut total) = (0, 0, 0);
    for term in terms.iter().filter(|t| t.chars().count() >= 4) {
        let all = forms(term);
        let others = &all[1..];
        before += right(engine.as_ref(), others);
        let (keys, _) = &all[0];
        for _ in 0..2 {
            engine.learn(keys, Language::Mixed, None);
        }
        after += right(engine.as_ref(), others);
        total += others.len();
    }
    let pct = |n: usize| 100.0 * n as f64 / total.max(1) as f64;
    report
        .metrics
        .push(("big.learn.other_forms_before".into(), pct(before)));
    report
        .metrics
        .push(("big.learn.other_forms_after".into(), pct(after)));
    let _ = fs::remove_file(&store);
}

/// All big sets. None when lab/data is missing.
pub fn run(engine: &dyn DecisionEngine, sample: usize, shown: usize) -> Option<Report> {
    let (Some(ko), Some(en), Some(titles)) = (
        read_lines("ko_sentences.txt"),
        read_lines("en_sentences.txt"),
        read_lines("ko_titles.txt"),
    ) else {
        println!("(no lab/data: run `python lab/fetch_data.py` for the big sets)\n");
        return None;
    };
    let terms: Vec<&str> = TERMS
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let mut report = Report {
        metrics: Vec::new(),
        misses: Vec::new(),
    };
    let ko = spread(&ko, sample);
    let pairs = |items: &[&String]| -> Vec<(String, String)> {
        items
            .iter()
            .map(|s| (keys_of_text(s), s.to_string()))
            .collect()
    };
    sentences(engine, "ko.sent", &pairs(&ko), shown, &mut report);
    let en = spread(&en, sample);
    let en_pairs: Vec<(String, String)> =
        en.iter().map(|s| (s.to_string(), s.to_string())).collect();
    sentences(engine, "en.sent", &en_pairs, shown, &mut report);

    let mut rng = Rng(7);
    let mut by_kind: std::collections::BTreeMap<&str, Vec<(String, String)>> = Default::default();
    for (kind, text) in code_switch(&ko, &terms, &mut rng) {
        by_kind
            .entry(kind)
            .or_default()
            .push((keys_of_text(&text), text));
    }
    for (kind, items) in &by_kind {
        sentences(engine, &format!("cs.{kind}"), items, shown, &mut report);
    }
    words(
        engine,
        "ko.title",
        &spread(&titles, sample),
        shown,
        &mut report,
    );
    learning(&terms, &mut report);
    Some(report)
}
