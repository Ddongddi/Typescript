//! Measurement lab for the English/Korean decision: accuracy per bucket (with sentence context),
//! decide latency and heap, every run compared with `baseline.tsv`.
//!
//!   cargo run --release --manifest-path lab/Cargo.toml                  report + diff
//!   cargo run --release --manifest-path lab/Cargo.toml -- --save        also record as baseline
//!   cargo run --release --manifest-path lab/Cargo.toml -- --misses 20   misses per bucket (5)
//!   cargo run --release --manifest-path lab/Cargo.toml -- --fold 1/3    only every 3rd sentence of
//!                                                                       the big sets (fold 1 of 3)
//!
//! Embedded packs only and no learning file, so a run means the same on every machine.
//! Cases: generated from crates/core/data (sampled across each whole list, not just the top),
//! plus cases/words.tsv and cases/sentences.tsv, which hold what *should* happen.

mod corpus;
mod scenario;
mod tune;

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::{Duration, Instant};

use typescript_core::lang::hangul::DubeolsikLayout;
use typescript_core::{
    make_engine, DecisionContext, DecisionEngine, Language, Languages, TextConverter,
};

const KOREAN_WORDS: &str = include_str!("../../crates/core/data/korean_words.txt");
const KOREAN_VOCAB: &str = include_str!("../../crates/core/data/korean_vocabulary.txt");
const ENGLISH_FREQ: &str = include_str!("../../crates/core/data/english_frequency.txt");
const WORDS: &str = include_str!("../cases/words.tsv");
const SENTENCES: &str = include_str!("../cases/sentences.tsv");
const CODE_SWITCH: &str = include_str!("../cases/codeswitch.tsv");

/// Words per generated bucket, and how deep into the English frequency list they come from.
const SAMPLE: usize = 2000;
const ENGLISH_DEPTH: usize = 20_000;
const DEFAULT_MISSES: usize = 5;
/// Items per big set (`--corpus N`); the sets in lab/data hold tens of thousands.
const DEFAULT_CORPUS: usize = 3000;
/// The lab's own version (lab/CHANGELOG.md): a baseline from another one may measure other sets.
const LAB_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Counts live heap bytes, so "lighter" is a number.
struct Counting;
static HEAP: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged and only counts sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            let now = HEAP.fetch_add(layout.size(), Relaxed) + layout.size();
            PEAK.fetch_max(now, Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        HEAP.fetch_sub(layout.size(), Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

struct Case {
    bucket: String,
    keys: String,
    previous: Option<Language>,
    expected: String,
}

impl Case {
    fn id(&self) -> String {
        format!(
            "{}\t{}\t{}",
            self.bucket,
            self.keys,
            prev_name(self.previous)
        )
    }
}

fn prev_name(previous: Option<Language>) -> &'static str {
    previous.map_or("-", Language::name)
}

const CHO: &str = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ";
const JUNG: [&str; 21] = [
    "ㅏ", "ㅐ", "ㅑ", "ㅒ", "ㅓ", "ㅔ", "ㅕ", "ㅖ", "ㅗ", "ㅗㅏ", "ㅗㅐ", "ㅗㅣ", "ㅛ", "ㅜ",
    "ㅜㅓ", "ㅜㅔ", "ㅜㅣ", "ㅠ", "ㅡ", "ㅡㅣ", "ㅣ",
];
const JONG: [&str; 28] = [
    "", "ㄱ", "ㄲ", "ㄱㅅ", "ㄴ", "ㄴㅈ", "ㄴㅎ", "ㄷ", "ㄹ", "ㄹㄱ", "ㄹㅁ", "ㄹㅂ", "ㄹㅅ",
    "ㄹㅌ", "ㄹㅍ", "ㄹㅎ", "ㅁ", "ㅂ", "ㅂㅅ", "ㅅ", "ㅆ", "ㅇ", "ㅈ", "ㅊ", "ㅋ", "ㅌ", "ㅍ",
    "ㅎ",
];

fn key_of(jamo: char) -> char {
    ('A'..='z')
        .find(|k| {
            DubeolsikLayout::jamo(*k) == Some(jamo) && (k.is_lowercase() || "QWERTOP".contains(*k))
        })
        .expect("every jamo has a key")
}

/// The keys that type `hangul` (syllables only) on a 2-set layout; same as core's tests/common.
fn keys_of(hangul: &str) -> String {
    let mut keys = String::new();
    for c in hangul.chars() {
        let i = c as usize - '가' as usize;
        keys.push(key_of(CHO.chars().nth(i / 588).unwrap()));
        JUNG[i % 588 / 28]
            .chars()
            .chain(JONG[i % 28].chars())
            .for_each(|j| keys.push(key_of(j)));
    }
    keys
}

/// Evenly spread over the whole list: the top words alone are all right already.
fn spread(items: Vec<&str>) -> Vec<&str> {
    let step = (items.len() / SAMPLE).max(1);
    items.into_iter().step_by(step).take(SAMPLE).collect()
}

fn korean_list(text: &str) -> Vec<&str> {
    spread(
        text.lines()
            .filter_map(|l| l.split('\t').next())
            .filter(|w| w.chars().count() >= 2 && w.chars().all(|c| ('가'..='힣').contains(&c)))
            .collect(),
    )
}

fn english_list() -> Vec<&'static str> {
    spread(
        ENGLISH_FREQ
            .lines()
            .take(ENGLISH_DEPTH)
            .filter_map(|l| l.split_whitespace().next())
            .filter(|w| w.len() >= 2 && w.bytes().all(|b| b.is_ascii_lowercase()))
            .collect(),
    )
}

fn data_lines(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect())
}

fn parse_previous(field: &str) -> Option<Language> {
    match field {
        "-" => None,
        other => Some(Language::parse(other).unwrap_or_else(|| panic!("bad previous: {other}"))),
    }
}

/// Word cases (one decide each).
fn word_cases() -> Vec<Case> {
    let contexts = [
        (None, "none"),
        (Some(Language::Korean), "after-ko"),
        (Some(Language::English), "after-en"),
    ];
    let mut cases = Vec::new();
    let generated = [
        ("ko.words", korean_list(KOREAN_WORDS), true),
        ("ko.vocab", korean_list(KOREAN_VOCAB), true),
        ("en.freq", english_list(), false),
    ];
    for (name, words, korean) in generated {
        for (previous, label) in contexts {
            for word in &words {
                let keys = if korean {
                    keys_of(word)
                } else {
                    word.to_string()
                };
                cases.push(Case {
                    bucket: format!("{name}/{label}"),
                    keys,
                    previous,
                    expected: word.to_string(),
                });
            }
        }
    }
    for f in data_lines(WORDS) {
        assert_eq!(f.len(), 4, "words.tsv: {f:?}");
        cases.push(Case {
            bucket: format!("hand.{}", f[3]),
            keys: f[0].to_string(),
            previous: parse_previous(f[1]),
            expected: f[2].to_string(),
        });
    }
    cases
}

/// The keys that type `text`: Hangul on a 2-set layout, everything else as written.
fn keys_of_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '가'..='힣' => keys_of(&c.to_string()),
            'ㄱ'..='ㅣ' => key_of(c).to_string(),
            other => other.to_string(),
        })
        .collect()
}

fn sentence_cases() -> Vec<Case> {
    let hand = data_lines(SENTENCES).map(|f| {
        assert_eq!(f.len(), 2, "sentences.tsv: {f:?}");
        Case {
            bucket: "sentences".to_string(),
            keys: f[0].to_string(),
            previous: None,
            expected: f[1].to_string(),
        }
    });
    let code_switch = data_lines(CODE_SWITCH).map(|f| {
        assert_eq!(f.len(), 2, "codeswitch.tsv: {f:?}");
        Case {
            bucket: format!("cs.{}", f[1]),
            keys: keys_of_text(f[0]),
            previous: None,
            expected: f[0].to_string(),
        }
    });
    hand.chain(code_switch).collect()
}

/// Levenshtein distance in characters (two rows).
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != *cb)).min(up + 1).min(row[j] + 1);
            diagonal = up;
        }
    }
    row[b.len()]
}

fn micros(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

/// avg, p50, p95, p99 in µs.
fn latency(metrics: &mut BTreeMap<String, f64>, name: &str, mut times: Vec<Duration>) {
    times.sort();
    let at = |q: f64| micros(times[((times.len() - 1) as f64 * q) as usize]);
    let avg = micros(times.iter().sum::<Duration>()) / times.len() as f64;
    metrics.insert(format!("{name}.avg_us"), avg);
    metrics.insert(format!("{name}.p50_us"), at(0.50));
    metrics.insert(format!("{name}.p95_us"), at(0.95));
    metrics.insert(format!("{name}.p99_us"), at(0.99));
}

/// Metrics plus the cases that failed (only those are stored; a case not listed passed).
struct Baseline {
    /// The lab version that recorded it (none: before the lab had versions).
    lab: Option<String>,
    metrics: BTreeMap<String, f64>,
    failed: std::collections::BTreeSet<String>,
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baseline.tsv")
}

fn read_baseline() -> Option<Baseline> {
    let text = std::fs::read_to_string(baseline_path()).ok()?;
    let mut baseline = Baseline {
        lab: None,
        metrics: BTreeMap::new(),
        failed: Default::default(),
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f.as_slice() {
            ["#lab", version] => baseline.lab = Some(version.to_string()),
            ["#metric", name, value] => {
                if let Ok(v) = value.parse() {
                    baseline.metrics.insert(name.to_string(), v);
                }
            }
            [bucket, keys, prev, _expected, _got] => {
                baseline.failed.insert(format!("{bucket}\t{keys}\t{prev}"));
            }
            _ => {}
        }
    }
    Some(baseline)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--tune") {
        tune::run();
        return;
    }
    // `--trace TEXT [--all] [--before-digit] [--after-digit]`: how the code-switch search
    // scores TEXT (given as it should come out).
    if let Some(text) = args.iter().position(|a| a == "--trace").and_then(|i| args.get(i + 1)) {
        let search = typescript_core::decide::codeswitch::CodeSwitch::new(
            typescript_core::EnglishModel::shared(),
            Default::default(),
        );
        let context = DecisionContext {
            after_digit: args.iter().any(|a| a == "--after-digit"),
            before_digit: args.iter().any(|a| a == "--before-digit"),
            ..Default::default()
        };
        let all = args.iter().any(|a| a == "--all");
        println!("{}", search.trace(&keys_of_text(text), &context, all));
        return;
    }
    // `--cold KEYS`: what deciding KEYS costs when nothing of it is remembered (the first key of a
    // token that was skipped until now), by alternating it with a token that shares nothing.
    if let Some(keys) = args.iter().position(|a| a == "--cold").and_then(|i| args.get(i + 1)) {
        let engine = make_engine(None, &Languages::legacy(false));
        let context = DecisionContext {
            partial: true,
            ..Default::default()
        };
        let other = "zmfltmxldksdlqsle";
        for _ in 0..2000 {
            engine.decide(other, &context);
            engine.decide(keys, &context);
        }
        let mut spent = Duration::ZERO;
        for _ in 0..20000 {
            engine.decide(other, &context);
            let t = Instant::now();
            engine.decide(keys, &context);
            spent += t.elapsed();
        }
        println!("cold {keys}: {:.1} µs", micros(spent) / 20000.0);
        return;
    }
    // `--event WORD TAIL`: what the key after an English word costs when it turns the token into
    // something to search (github + d): the search of another token is what the cache holds, and
    // the word's own walk is warm, as it is while typing.
    if let Some(i) = args.iter().position(|a| a == "--event") {
        let (word, tail) = (&args[i + 1], &args[i + 2]);
        let engine = make_engine(None, &Languages::legacy(false));
        let context = DecisionContext {
            partial: true,
            ..Default::default()
        };
        let (other, whole) = ("zmfltmxldksdlqsle", format!("{word}{tail}"));
        let mut spent = Duration::ZERO;
        for round in 0..22000 {
            engine.decide(other, &context);
            engine.decide(word, &context);
            let t = Instant::now();
            engine.decide(&whole, &context);
            if round >= 2000 {
                spent += t.elapsed();
            }
        }
        println!("event {word}+{tail}: {:.1} µs", micros(spent) / 20000.0);
        return;
    }
    // `--type KEYS`: what each key costs while KEYS is typed one key at a time (mean of many
    // times over, the token and all after the one before it).
    if let Some(keys) = args.iter().position(|a| a == "--type").and_then(|i| args.get(i + 1)) {
        let engine = make_engine(None, &Languages::legacy(false));
        let previous = args
            .iter()
            .position(|a| a == "--prev")
            .and_then(|i| args.get(i + 1))
            .map(|l| if l == "ko" { Language::Korean } else { Language::English });
        let context = DecisionContext {
            partial: true,
            previous_language: previous,
            ..Default::default()
        };
        let ends: Vec<usize> = keys.char_indices().skip(1).map(|(end, _)| end).collect();
        let mut spent = vec![Duration::ZERO; ends.len()];
        // `--noisy`: the words of the lab are typed between the rounds, which cools the caches.
        let noise = if args.iter().any(|a| a == "--noisy") {
            word_cases()
        } else {
            Vec::new()
        };
        let mut next = 0;
        for round in 0..6000 {
            for _ in 0..(noise.len() > 0) as usize * 40 {
                let c = &noise[next % noise.len()];
                next += 7;
                for (end, _) in c.keys.char_indices().skip(1) {
                    engine.decide(&c.keys[..end], &context);
                }
            }
            for (slot, end) in ends.iter().enumerate() {
                let t = Instant::now();
                engine.decide(&keys[..*end], &context);
                if round >= 1000 {
                    spent[slot] += t.elapsed();
                }
            }
        }
        let line: Vec<String> = spent.iter().map(|d| format!("{:.1}", micros(*d) / 5000.0)).collect();
        println!("type {keys}: {}", line.join(" "));
        return;
    }
    // `--probe FILE`: convert each line (the text as it should come out) and print what came out.
    if let Some(path) = args.iter().position(|a| a == "--probe").and_then(|i| args.get(i + 1)) {
        let engine = make_engine(None, &Languages::legacy(false));
        let converter = TextConverter::new(engine.as_ref());
        let text = std::fs::read_to_string(path).expect("read probe file");
        // `--time`: what one more key costs while the token is this long (the live preview).
        if args.iter().any(|a| a == "--time") {
            let keys = keys_of_text(text.lines().next().unwrap_or(""));
            let context = DecisionContext {
                partial: true,
                ..Default::default()
            };
            for len in [50, 100, 200, 400, 800, 1600] {
                let Some((end, _)) = keys.char_indices().nth(len) else { break };
                // The key before, as typed a moment ago; then the timed key.
                let (before, _) = keys.char_indices().nth(len - 1).unwrap();
                engine.decide(&keys[..before], &context);
                let t = Instant::now();
                engine.decide(&keys[..end], &context);
                println!("{len:5} keys  {:9.1} µs", micros(t.elapsed()));
            }
            return;
        }
        for line in text.lines().filter(|l| !l.is_empty()) {
            println!("{}", converter.convert(&keys_of_text(line)).0);
        }
        return;
    }
    let save = args.iter().any(|a| a == "--save");
    // `--fold K/N`: the big sets keep only the lines whose number is K modulo N, so what a choice
    // is made on (one fold) and what it is checked on (another) share no sentence.
    let fold: Option<(usize, usize)> = args
        .iter()
        .position(|a| a == "--fold")
        .and_then(|i| args.get(i + 1)?.split_once('/'))
        .and_then(|(k, n)| Some((k.parse().ok()?, n.parse().ok()?)))
        .filter(|(k, n)| k < n);
    let shown = args
        .iter()
        .position(|a| a == "--misses")
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(DEFAULT_MISSES);

    let mut metrics: BTreeMap<String, f64> = BTreeMap::new();

    // Cold start: engine plus the lazy tables the first words pull in.
    let heap_before = HEAP.load(Relaxed);
    let start = Instant::now();
    let engine = make_engine(None, &Languages::legacy(false));
    for keys in ["dkssud", "hello"] {
        engine.decide(keys, &DecisionContext::default());
    }
    metrics.insert("load.ms".into(), start.elapsed().as_secs_f64() * 1e3);
    metrics.insert(
        "heap.loaded_kb".into(),
        HEAP.load(Relaxed).saturating_sub(heap_before) as f64 / 1024.0,
    );

    let words = word_cases();
    let sentences = sentence_cases();
    let sentence_count = sentences.len();
    let context = |previous, partial| DecisionContext {
        previous_language: previous,
        partial,
        ..Default::default()
    };

    // Warm pass, then the timed one (its outputs are the results).
    for c in &words {
        engine.decide(&c.keys, &context(c.previous, false));
    }
    // `--steady`: one untimed pass over every prefix first, so the timings leave out what is
    // looked up for the first time (the caches that fill as words come up).
    if args.iter().any(|a| a == "--steady") {
        for c in &words {
            for (end, _) in c.keys.char_indices().skip(1) {
                engine.decide(&c.keys[..end], &context(c.previous, true));
            }
        }
    }
    let (mut commit, mut keystroke) = (Vec::new(), Vec::new());
    let mut slowest: Vec<(Duration, String)> = Vec::new();
    let track_slowest = args.iter().any(|a| a == "--slow");
    let mut by_len: std::collections::BTreeMap<usize, Vec<Duration>> = Default::default();
    let mut got: Vec<String> = Vec::with_capacity(words.len() + sentences.len());
    for c in &words {
        // Every prefix, as the live preview sees it while typing.
        for (end, _) in c.keys.char_indices().skip(1) {
            let t = Instant::now();
            engine.decide(&c.keys[..end], &context(c.previous, true));
            let spent = t.elapsed();
            keystroke.push(spent);
            by_len.entry(end).or_default().push(spent);
            if track_slowest {
                slowest.push((spent, c.keys[..end].to_string()));
            }
        }
        let t = Instant::now();
        let d = engine.decide(&c.keys, &context(c.previous, false));
        commit.push(t.elapsed());
        got.push(d.output);
    }
    let converter = TextConverter::new(engine.as_ref());
    got.extend(sentences.iter().map(|c| converter.convert(&c.keys).0));
    slowest.sort_by(|a, b| b.0.cmp(&a.0));
    for (spent, keys) in slowest.iter().take(25) {
        println!("slow keystroke {:7.1}µs  {keys}", micros(*spent));
    }
    // `--bylen`: how long one more key takes while the token is this long.
    if args.iter().any(|a| a == "--bylen") {
        for (len, spent) in &mut by_len {
            spent.sort();
            let mean = spent.iter().map(|d| micros(*d)).sum::<f64>() / spent.len() as f64;
            println!("bylen {len:3} n={:6} mean {mean:6.1}µs  p99 {:6.1}µs", spent.len(), micros(spent[spent.len() * 99 / 100]));
        }
    }
    latency(&mut metrics, "commit", commit);
    latency(&mut metrics, "keystroke", keystroke);
    metrics.insert("heap.peak_kb".into(), PEAK.load(Relaxed) as f64 / 1024.0);

    // The big sets come after the heap numbers: they allocate a lot.
    let sample = args
        .iter()
        .position(|a| a == "--corpus")
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(DEFAULT_CORPUS);
    let big = if args.iter().any(|a| a == "--no-corpus") {
        None
    } else {
        corpus::run(engine.as_ref(), sample, shown, fold)
    };
    let scenarios = scenario::run(engine.as_ref(), shown);
    for report in big.iter().chain([&scenarios]) {
        metrics.extend(report.metrics.iter().cloned());
    }

    let cases: Vec<Case> = words.into_iter().chain(sentences).collect();
    let mut buckets: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (c, g) in cases.iter().zip(&got) {
        let entry = buckets.entry(&c.bucket).or_default();
        entry.1 += 1;
        if *g == c.expected {
            entry.0 += 1;
        }
    }
    let total_ok: usize = buckets.values().map(|b| b.0).sum();
    for (bucket, (ok, n)) in &buckets {
        metrics.insert(format!("acc.{bucket}"), 100.0 * *ok as f64 / *n as f64);
    }
    metrics.insert(
        "acc.ALL".into(),
        100.0 * total_ok as f64 / cases.len() as f64,
    );

    // Per character: a long story with one wrong letter is 99.9 %, not a miss.
    let (mut chars, mut edits) = (0usize, 0usize);
    for (c, g) in cases.iter().zip(&got).skip(cases.len() - sentence_count) {
        chars += c.expected.chars().count();
        edits += edit_distance(&c.expected, g);
    }
    metrics.insert(
        "chars.sentences".into(),
        100.0 * (1.0 - edits as f64 / chars.max(1) as f64),
    );

    let baseline = read_baseline();
    print_report(&metrics, &buckets, baseline.as_ref());
    print_changes(&cases, &got, baseline.as_ref());
    print_misses(&cases, &got, shown);
    for report in big.iter().chain([&scenarios]) {
        for (bucket, misses) in report.misses.iter().filter(|(_, m)| !m.is_empty()) {
            println!("\n  {bucket} (first {})", misses.len());
            misses.iter().for_each(|m| println!("    {m}"));
        }
    }

    if save {
        let mut out = format!("#lab\t{LAB_VERSION}\n");
        for (name, value) in &metrics {
            writeln!(out, "#metric\t{name}\t{value:.3}").unwrap();
        }
        for (c, g) in cases.iter().zip(&got).filter(|(c, g)| **g != c.expected) {
            writeln!(out, "{}\t{}\t{}", c.id(), c.expected, g).unwrap();
        }
        std::fs::write(baseline_path(), out).expect("write baseline.tsv");
        println!("\nsaved {}", baseline_path().display());
    }
}

fn print_report(
    metrics: &BTreeMap<String, f64>,
    buckets: &BTreeMap<&str, (usize, usize)>,
    baseline: Option<&Baseline>,
) {
    match baseline.map(|b| b.lab.as_deref()) {
        None => println!("lab {LAB_VERSION}\n(no baseline.tsv yet: run with --save to record one)\n"),
        Some(Some(v)) if v == LAB_VERSION => println!("lab {LAB_VERSION}\n"),
        Some(v) => println!(
            "lab {LAB_VERSION} (the baseline is from lab {}: its sets may differ)\n",
            v.unwrap_or("before versions")
        ),
    }
    println!(
        "{:<34} {:>10} {:>10} {:>9}",
        "metric", "now", "baseline", "Δ"
    );
    for (name, value) in metrics {
        let base = baseline.and_then(|b| b.metrics.get(name)).copied();
        let n = name
            .strip_prefix("acc.")
            .and_then(|b| buckets.get(b))
            .map_or(String::new(), |(_, n)| format!("  n={n}"));
        match base {
            Some(b) => println!("{name:<34} {value:>10.2} {b:>10.2} {:>+9.2}{n}", value - b),
            None => println!("{name:<34} {value:>10.2} {:>10} {:>9}{n}", "-", "-"),
        }
    }
    println!("\nacc: % right · times: µs per decide (keystroke = partial prefixes) · heap: KiB");
}

/// Cases that flipped since the baseline: what a change fixed and what it broke.
fn print_changes(cases: &[Case], got: &[String], baseline: Option<&Baseline>) {
    let Some(baseline) = baseline else { return };
    let (mut fixed, mut broke) = (Vec::new(), Vec::new());
    for (c, g) in cases.iter().zip(got) {
        let (was, now) = (!baseline.failed.contains(&c.id()), *g == c.expected);
        if now && !was {
            fixed.push(format!("{} → {g}", c.id()));
        } else if was && !now {
            broke.push(format!("{} → {g} (want {})", c.id(), c.expected));
        }
    }
    println!("\nfixed {} · broke {}", fixed.len(), broke.len());
    for l in &fixed {
        println!("  + {l}");
    }
    for l in &broke {
        println!("  - {l}");
    }
}

fn print_misses(cases: &[Case], got: &[String], shown: usize) {
    let mut by_bucket: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (c, g) in cases.iter().zip(got) {
        if *g != c.expected {
            by_bucket.entry(&c.bucket).or_default().push(format!(
                "{} [{}] → {g} (want {})",
                c.keys,
                prev_name(c.previous),
                c.expected
            ));
        }
    }
    if shown == 0 || by_bucket.is_empty() {
        return;
    }
    println!("\nmisses (first {shown} per bucket):");
    for (bucket, misses) in by_bucket {
        println!("  {bucket} ({})", misses.len());
        for m in misses.iter().take(shown) {
            println!("    {m}");
        }
    }
}
