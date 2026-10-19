//! The big sets: real sentences and words from lab/data (fetched by lab/fetch_data.py, never
//! committed), synthetic code-switching built from them, and a learning simulation.
//!
//! Every Korean item is typed back to QWERTY keys (`keys_of_text`), so the right answer is the
//! item itself. Without lab/data the lab prints a hint and skips all of this.

use std::fs;
use std::path::PathBuf;

use tacit_core::{DecisionContext, DecisionEngine, Language, Languages, TextConverter};

use crate::{keys_of, keys_of_text, make_engine};

const TERMS: &str = include_str!("../cases/terms.txt");
const SHORT_TERMS: &str = include_str!("../cases/short_terms.txt");
/// The pack generator's source: its extra-word list says which terms were tuned for.
const PACKGEN: &str = include_str!("../../crates/packgen/src/lib.rs");

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
pub fn token_hits(expected: &str, got: &str) -> (usize, usize) {
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
            ..Default::default()
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
fn learning(terms: &[&str], languages: &Languages, report: &mut Report) {
    let endings = ["이", "을", "에", "에서", "으로", "는", "와", "도"];
    let store = std::env::temp_dir().join(format!("tacit-lab-{}.json", std::process::id()));
    let _ = fs::remove_file(&store);
    let engine = make_engine(Some(store.clone()), languages);
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

/// The language an Esc correction to `want` teaches.
fn taught_language(keys: &str, want: &str) -> Language {
    if want == keys {
        Language::English
    } else if want.chars().any(|c| c.is_ascii_alphabetic()) {
        Language::Mixed
    } else {
        Language::Korean
    }
}

/// Esc generalization: correct half of the engine's misses twice (as a user would), then look
/// at the *other* items of the same set. `fixed` = share of the unseen misses now right,
/// `broken` = items right before that are wrong after, `net` = fixed count - broken count.
/// An engine that only remembers the exact keys scores about 0 here.
fn transfer(
    name: &str,
    languages: &Languages,
    items: &[(String, String, String)],
    controls: &[(String, String)],
    context: &DecisionContext,
    report: &mut Report,
) {
    let store = std::env::temp_dir().join(format!("tacit-lab-t-{}.json", std::process::id()));
    let _ = fs::remove_file(&store);
    let engine = make_engine(Some(store.clone()), languages);
    let right = |engine: &dyn DecisionEngine, (_, keys, want): &(String, String, String)| {
        engine.decide(keys, context).output == *want
    };
    let (misses, hits): (Vec<_>, Vec<_>) = items.iter().partition(|i| !right(engine.as_ref(), i));
    // Words of the other language that must stay as they are, whatever was taught.
    let control_ok = |engine: &dyn DecisionEngine| -> Vec<bool> {
        controls
            .iter()
            .map(|(k, w)| right(engine, &(String::new(), k.clone(), w.clone())))
            .collect()
    };
    let control_before = control_ok(engine.as_ref());
    // Whole groups (a term in all its forms) are taught or unseen: a form of a taught group
    // is no test of generalization.
    let mut groups: Vec<&str> = misses.iter().map(|(g, _, _)| g.as_str()).collect();
    groups.dedup();
    let taught_group = |g: &str| {
        groups
            .iter()
            .position(|x| *x == g)
            .is_some_and(|i| i % 2 == 0)
    };
    let unseen: Vec<_> = misses.iter().filter(|(g, _, _)| !taught_group(g)).collect();
    for (_, keys, want) in misses.iter().filter(|(g, _, _)| taught_group(g)) {
        let language = taught_language(keys, want);
        for _ in 0..2 {
            engine.learn(keys, language, None);
        }
    }
    let fixed = unseen.iter().filter(|i| right(engine.as_ref(), i)).count();
    let control_after = control_ok(engine.as_ref());
    let broken_controls = control_before
        .iter()
        .zip(&control_after)
        .filter(|(before, after)| **before && !**after)
        .count();
    let broken = hits.iter().filter(|i| !right(engine.as_ref(), i)).count() + broken_controls;
    let hits_total = hits.len() + control_before.iter().filter(|b| **b).count();
    let pct = |n: usize, of: usize| 100.0 * n as f64 / of.max(1) as f64;
    for (metric, value) in [
        ("fixed", pct(fixed, unseen.len())),
        ("broken", pct(broken, hits_total)),
        ("net", fixed as f64 - broken as f64),
        ("misses", misses.len() as f64),
    ] {
        report
            .metrics
            .push((format!("big.transfer.{name}.{metric}"), value));
    }
    let _ = fs::remove_file(&store);
}

/// All big sets. None when lab/data is missing.
/// The lines of fold `k` of `n` (every n-th, from k): the folds of one list share nothing.
fn part(items: Vec<String>, fold: Option<(usize, usize)>) -> Vec<String> {
    let Some((k, n)) = fold else { return items };
    items.into_iter().skip(k).step_by(n).collect()
}

/// Separate diagnostic run: regular accuracy/latency measurements stay unchanged.
pub fn audit(
    engine: &dyn DecisionEngine,
    sample: usize,
    shown: usize,
    fold: Option<(usize, usize)>,
) {
    let ko = part(
        read_lines("ko_sentences.txt").expect("run python lab/fetch_data.py first"),
        fold,
    );
    let ko = spread(&ko, sample);
    crate::audit::sentences(
        engine,
        "ko.sent",
        &ko.iter().map(|s| (*s).clone()).collect::<Vec<_>>(),
        shown,
    );
    let en = part(
        read_lines("en_sentences.txt").expect("English corpus required"),
        fold,
    );
    crate::audit::sentences(
        engine,
        "en.sent",
        &spread(&en, sample).into_iter().cloned().collect::<Vec<_>>(),
        shown,
    );
    for (name, source, seed) in [("cs", TERMS, 7), ("cs_short", SHORT_TERMS, 11)] {
        let terms: Vec<_> = source
            .lines()
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
            .collect();
        let mut sets: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
        for (kind, text) in code_switch(&ko, &terms, &mut Rng(seed)) {
            sets.entry(kind).or_default().push(text);
        }
        for (kind, texts) in sets {
            crate::audit::sentences(engine, &format!("{name}.{kind}"), &texts, shown);
        }
    }
}

pub fn run(
    engine: &dyn DecisionEngine,
    languages: &Languages,
    sample: usize,
    shown: usize,
    fold: Option<(usize, usize)>,
) -> Option<Report> {
    let (Some(ko), Some(en), Some(titles)) = (
        read_lines("ko_sentences.txt"),
        read_lines("en_sentences.txt"),
        read_lines("ko_titles.txt"),
    ) else {
        println!("(no lab/data: run `python lab/fetch_data.py` for the big sets)\n");
        return None;
    };
    if let Some((k, n)) = fold {
        println!("fold {k}/{n}: the big sets hold every {n}th line from {k}; baseline diffs are not like for like\n");
    }
    let (ko, en, titles) = (part(ko, fold), part(en, fold), part(titles, fold));
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
    if let Some(phrases) = read_lines("ko_phrases.txt").map(|l| part(l, fold)) {
        sentences(
            engine,
            "ko.phrase",
            &pairs(&spread(&phrases, sample)),
            shown,
            &mut report,
        );
    }
    if let Some(comments) = read_lines("ko_comments.txt").map(|l| part(l, fold)) {
        // News comments: informal text from another place than the sentences above.
        sentences(
            engine,
            "ko.comment",
            &pairs(&spread(&comments, sample)),
            shown,
            &mut report,
        );
    }
    if let Some(long) = read_lines("ko_long.txt").map(|l| part(l, fold)) {
        sentences(
            engine,
            "ko.long",
            &pairs(&spread(&long, sample.min(1000))),
            shown,
            &mut report,
        );
    }
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
    // Short English words that are also whole Korean syllables, put into the same sentences:
    // they have to stay English.
    let short: Vec<&str> = SHORT_TERMS
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let bare: Vec<(String, String)> = code_switch(&ko, &short, &mut Rng(11))
        .into_iter()
        .filter(|(kind, _)| *kind == "bare")
        .map(|(_, text)| (keys_of_text(&text), text))
        .collect();
    sentences(engine, "cs_short.bare", &bare, shown, &mut report);
    // Terms the pack's extra-word list does not name: what the engine does on names nobody
    // listed (the honest number once a word list was tuned on terms.txt).
    let listed: std::collections::HashSet<&str> = PACKGEN.split_whitespace().collect();
    let held_out: Vec<&str> = terms
        .iter()
        .copied()
        .filter(|t| !listed.contains(t.to_lowercase().as_str()))
        .collect();
    let mut by_kind: std::collections::BTreeMap<&str, Vec<(String, String)>> = Default::default();
    for (kind, text) in code_switch(&ko, &held_out, &mut Rng(7)) {
        by_kind
            .entry(kind)
            .or_default()
            .push((keys_of_text(&text), text));
    }
    for (kind, items) in by_kind.iter().filter(|(k, _)| **k == "particle") {
        sentences(
            engine,
            &format!("cs_heldout.{kind}"),
            items,
            shown,
            &mut report,
        );
    }
    words(
        engine,
        "ko.title",
        &spread(&titles, sample),
        shown,
        &mut report,
    );
    learning(&terms, languages, &mut report);
    // Esc generalization: other terms with a particle, and Korean words after English.
    let particle_items: Vec<(String, String, String)> = held_out
        .iter()
        .filter(|t| t.chars().count() >= 4)
        .flat_map(|t| {
            ["가", "에서", "으로", "를"].iter().map(move |p| {
                (
                    t.to_string(),
                    format!("{t}{}", keys_of(p)),
                    format!("{t}{p}"),
                )
            })
        })
        .collect();
    // Plain English words, to see that nothing taught pulls them to Korean.
    let english_words: Vec<(String, String)> = {
        let mut seen = std::collections::BTreeSet::new();
        en.iter()
            .flat_map(|s| s.split(' ').map(str::to_lowercase).collect::<Vec<_>>())
            .filter(|w| (4..=8).contains(&w.len()) && w.bytes().all(|b| b.is_ascii_lowercase()))
            .filter(|w| seen.insert(w.clone()))
            .map(|w| (w.clone(), w))
            .collect()
    };
    transfer(
        "particle",
        languages,
        &particle_items,
        &english_words,
        &DecisionContext::default(),
        &mut report,
    );
    // Chat jamo: consonant-only keys, each its own group.
    let jamo_items: Vec<(String, String, String)> = [
        "ㄴㄴ", "ㅁㅊ", "ㄹㅇ", "ㅇㅈ", "ㅅㄱ", "ㄱㄱ", "ㅇㅋ", "ㅈㄴ", "ㅂㅅ", "ㅅㅂ", "ㅈㅅ",
        "ㄱㅅ", "ㅇㅇ", "ㅋㅋ", "ㅎㅎ", "ㄷㄷ", "ㅊㅊ", "ㅌㅌ", "ㄱㅊ", "ㅁㅈ", "ㅈㄹ", "ㅇㄷ",
        "ㅅㅅ", "ㅂㅂ",
    ]
    .iter()
    .map(|j| (j.to_string(), keys_of_text(j), j.to_string()))
    .collect();
    let abbreviations: Vec<(String, String)> = [
        "pm", "hd", "fx", "gb", "kb", "mb", "cpu", "gpu", "ssd", "hdd",
    ]
    .iter()
    .map(|w| (w.to_string(), w.to_string()))
    .collect();
    transfer(
        "jamo",
        languages,
        &jamo_items,
        &abbreviations,
        &DecisionContext::default(),
        &mut report,
    );
    let title_items: Vec<(String, String, String)> = pairs(&spread(&titles, sample))
        .into_iter()
        .map(|(keys, want)| (keys.clone(), keys, want))
        .collect();
    transfer(
        "title_after_en",
        languages,
        &title_items,
        &english_words,
        &DecisionContext {
            previous_language: Some(Language::English),
            ..Default::default()
        },
        &mut report,
    );
    Some(report)
}
