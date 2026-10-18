//! Golden snapshots of the decisions and session edits as they were before the language-module
//! refactor. Every later change must reproduce them exactly.
//!
//! Regenerate only on purpose (and with the user's agreement):
//!   GOLDEN_WRITE=1 cargo test --test golden
//!   GOLDEN_WRITE=1 cargo test --test golden --features tacit-core/japanese

mod common;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use common::keys_of;
use tacit_core::session::testing::FakeTextField;
use tacit_core::session::Range;
use tacit_core::{
    make_engine, AppPolicies, DecisionContext, DecisionEngine, History, InputSession, Key,
    Language, Languages, LearningEngine, TextClient,
};

const ENGLISH_FREQ: &str = include_str!("../data/english_frequency.txt");
const KOREAN_WORDS: &str = include_str!("../data/korean_words.txt");
const KOREAN_VOCAB: &str = include_str!("../data/korean_vocabulary.txt");
const WEB2: &str = include_str!("../data/web2.txt");
const SCRIPTS: &str = include_str!("golden/session_scripts.txt");

/// Hand-picked keys the corpora don't cover: splits, capitals, romaji, symbols.
const EXTRA: &[&str] = &[
    "githubdp",
    "slackdptj",
    "hellodnjfem",
    "UIrkxdms",
    "ThkT",
    "RoRmtgkgfu",
    "dkssudgktpdy",
    "kyouhaiitenkidesune",
    "kyouhagoogledekensaku",
    "githubde",
    "slackwo",
    "guswo",
    "ramen",
    "sake",
    "kite",
    "same",
    "ra-men",
    "kan'i",
    "Hello",
    "API",
    "VSCode",
    "to",
    "so",
    "go",
    "do",
    "a",
    "i",
    "dork",
    "ahem",
    "dkssud",
    "hello",
    "nihongo",
    "toukyou",
    "henkan",
    "honn",
];

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn check(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("GOLDEN_WRITE").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run with GOLDEN_WRITE=1 once)", path.display()));
    if expected != actual && !same_up_to_libm(&expected, actual) {
        let first = expected
            .lines()
            .zip(actual.lines())
            .find(|(e, a)| e != a)
            .map(|(e, a)| format!("expected: {e}\n  actual: {a}"))
            .unwrap_or_else(|| "line count differs".to_string());
        panic!("{name} differs from the golden snapshot\n{first}");
    }
}

/// Windows (where the snapshots were taken) must match byte for byte. Elsewhere the math library
/// rounds `exp`/`ln` a few digits differently, so numbers may differ by 1e-6; every other field
/// (languages, text, flags) must still be identical.
fn same_up_to_libm(expected: &str, actual: &str) -> bool {
    if cfg!(windows) || expected.lines().count() != actual.lines().count() {
        return false;
    }
    expected.lines().zip(actual.lines()).all(|(e, a)| {
        let (e, a): (Vec<_>, Vec<_>) = (e.split('\t').collect(), a.split('\t').collect());
        e.len() == a.len()
            && e.iter().zip(&a).all(|(e, a)| {
                e == a
                    || matches!((e.parse::<f64>(), a.parse::<f64>()),
                        (Ok(x), Ok(y)) if (x - y).abs() < 1e-6)
            })
    })
}

fn hangul_only(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| ('가'..='힣').contains(&c))
}

fn decide_inputs() -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    keys.extend(
        ENGLISH_FREQ
            .lines()
            .take(30_000)
            .step_by(21)
            .filter_map(|l| l.split_whitespace().next())
            .filter(|w| w.chars().all(|c| c.is_ascii_alphabetic()))
            .map(str::to_string),
    );
    keys.extend(
        KOREAN_WORDS
            .lines()
            .step_by(15)
            .filter_map(|l| l.split('\t').next())
            .filter(|w| hangul_only(w))
            .map(keys_of),
    );
    keys.extend(
        KOREAN_VOCAB
            .lines()
            .step_by(30)
            .map(str::trim)
            .filter(|w| hangul_only(w))
            .map(keys_of),
    );
    keys.extend(WEB2.lines().step_by(291).map(str::to_lowercase));
    keys.extend(EXTRA.iter().map(|s| s.to_string()));
    keys
}

/// The Esc cycle from `start`: every language Esc visits, in order, until it comes back.
fn cycle(start: Language, keys: &str, japanese: bool) -> String {
    let mut seen = vec![start];
    let mut current = start;
    for _ in 0..6 {
        current = current.next(keys, &Languages::legacy(japanese));
        if current == start {
            break;
        }
        seen.push(current);
    }
    seen.iter().map(|l| l.name()).collect::<Vec<_>>().join(",")
}

fn decide_snapshot(japanese: bool) -> String {
    let engine = make_engine(None, &Languages::legacy(japanese));
    let mut previous: Vec<Option<Language>> =
        vec![None, Some(Language::English), Some(Language::Korean)];
    if japanese {
        previous.push(Some(Language::Japanese));
    }
    let mut out = String::new();
    for keys in decide_inputs() {
        for prev in &previous {
            for partial in [false, true] {
                let ctx = DecisionContext {
                    previous_language: *prev,
                    partial,
                    ..Default::default()
                };
                let d = engine.decide(&keys, &ctx);
                writeln!(
                    out,
                    "{keys}\t{}\t{partial}\t{}\t{}\t{:?}\t{}\t{}",
                    prev.map_or("-", Language::name),
                    d.language.name(),
                    d.output,
                    d.confidence,
                    d.runner_up.map_or("-", Language::name),
                    cycle(d.language, &keys, japanese),
                )
                .unwrap();
            }
        }
    }
    out
}

#[test]
fn decisions_en_ko() {
    check("decide_enko.tsv", &decide_snapshot(false));
}

#[test]
#[cfg(feature = "embedded-ja")]
fn decisions_en_ko_ja() {
    check("decide_enkoja.tsv", &decide_snapshot(true));
}

/// Logs every edit the session makes, in order.
struct Recorder {
    field: FakeTextField,
    log: Vec<String>,
}

impl TextClient for Recorder {
    fn set_marked_text(&mut self, text: &str) {
        self.log.push(format!("M{text}"));
        self.field.set_marked_text(text);
    }
    fn insert_text(&mut self, text: &str, replacing: Option<Range>) {
        let at = replacing.map_or("-".to_string(), |r| format!("{}+{}", r.location, r.length));
        self.log.push(format!("I{text}@{at}"));
        self.field.insert_text(text, replacing);
    }
    fn selected_range(&self) -> Range {
        self.field.selected_range()
    }
    fn marked_range(&self) -> Range {
        self.field.marked_range()
    }
    fn text_in(&self, range: Range) -> Option<String> {
        self.field.text_in(range)
    }
}

fn session_for(config: &str) -> Option<InputSession> {
    let japanese = config.starts_with("enkoja");
    let engine: Arc<LearningEngine> = make_engine(None, &Languages::legacy(japanese));
    let mut session = InputSession::new(engine, AppPolicies::default(), true);
    if config == "enko-tab" {
        session = session.with_history(Arc::new(History::new(None)));
    }
    #[cfg(feature = "japanese")]
    if japanese {
        {
            let converter: Arc<dyn tacit_core::JapaneseConverter> =
                Arc::new(tacit_core::DictionaryConverter::new(None));
            session = session.with_japanese(converter, config == "enkoja-live");
        }
        if config == "enkoja-fw" {
            session.set_full_width_digits(true);
            session.set_full_width_space(true);
        }
    }
    Some(session)
}

fn key_for(ch: char) -> Key {
    match ch {
        '⏎' => Key::Enter,
        '⎋' => Key::Escape,
        '⇧' => Key::Cycle,
        '⌫' => Key::Delete,
        '⇥' => Key::Tab,
        '␠' => Key::ShiftSpace,
        '←' => Key::Left,
        '→' => Key::Right,
        '↑' => Key::Up,
        '↓' => Key::Down,
        'Ⓚ' => Key::Katakana,
        'Ⓗ' => Key::Hiragana,
        '▸' => Key::Other,
        c if c.is_ascii_alphabetic() => Key::Letter(c),
        c => Key::Printable(c),
    }
}

/// What the app itself types for a key the session passed on.
fn passed_on(key: Key) -> Option<String> {
    match key {
        Key::Enter => Some("\n".into()),
        Key::Printable(c) | Key::Letter(c) => Some(c.to_string()),
        Key::ShiftSpace => Some(" ".into()),
        Key::Tab => Some("\t".into()),
        _ => None,
    }
}

fn run_script(session: &mut InputSession, script: &str) -> (Vec<String>, String) {
    let mut rec = Recorder {
        field: FakeTextField::new(),
        log: Vec::new(),
    };
    for ch in script.chars() {
        let key = key_for(ch);
        if !session.handle(key, &mut rec) {
            rec.log.push(format!("P{ch}"));
            if let Some(text) = passed_on(key) {
                rec.field.insert_text(&text, None);
            }
        }
    }
    session.commit(&mut rec);
    let text = rec.field.text();
    (rec.log, text)
}

/// Generated scripts: each sampled word committed by Space, flipped, cycled and trimmed.
fn generated_scripts() -> Vec<(String, String)> {
    let mut scripts = Vec::new();
    for keys in decide_inputs().iter().step_by(40) {
        for config in ["enko", "enkoja"] {
            for pattern in ["{} ", "{}⎋ ", "{} ⎋", "{}⌫⌫ ", "{}⎋⎋⏎"] {
                scripts.push((config.to_string(), pattern.replace("{}", keys)));
            }
        }
    }
    scripts
}

fn session_snapshot(include: impl Fn(&str) -> bool) -> String {
    let hand = SCRIPTS.lines().filter(|l| !l.is_empty()).map(|l| {
        let (config, script) = l.split_once('\t').expect("config<TAB>script");
        (config.to_string(), script.to_string())
    });
    let mut out = String::new();
    for (config, script) in hand.chain(generated_scripts()) {
        if !include(&config) {
            continue;
        }
        // A fresh session per script: learning and history must not leak between lines.
        let Some(mut session) = session_for(&config) else {
            continue;
        };
        let (log, text) = run_script(&mut session, &script);
        writeln!(out, "{config}\t{script}\t{}\t{text:?}", log.join("|")).unwrap();
    }
    out
}

#[test]
fn session_edits_en_ko() {
    check(
        "session_enko.tsv",
        &session_snapshot(|c| !c.starts_with("enkoja")),
    );
}

#[test]
#[cfg(feature = "embedded-ja")]
fn session_edits_en_ko_ja() {
    check("session_enkoja.tsv", &session_snapshot(|_| true));
}
