//! Fits the weights of the code-switch search (`--tune`) by coordinate descent.
//!
//! Four token sets, every token judged by `CodeSwitch::decide` alone (no engine, no learning):
//! the hand-written cases and a synthetic set of English word + Korean ending/word should come
//! out cut as written; English words and Korean words must stay uncut. A fifth of every list is
//! held out and only reported at the end, so a fit that just memorised shows.

use typescript_core::decide::codeswitch::{CodeSwitch, Params};
use typescript_core::{DecisionContext, EnglishModel};

use super::{
    data_lines, english_list, keys_of_text, korean_list, CODE_SWITCH, KOREAN_VOCAB, KOREAN_WORDS,
    SENTENCES, WORDS,
};

/// Webster's Second: rare English words, the hardest ones to leave uncut.
const WEB2: &str = include_str!("../../crates/core/data/web2.txt");

/// Developer terms (names, acronyms, jargon): the English that no word list may know.
const TERMS: &str = include_str!("../cases/terms.txt");

/// Korean endings and words typed straight after an English word in the synthetic set.
const ENDINGS: [&str; 24] = [
    "에",
    "에서",
    "를",
    "을",
    "은",
    "는",
    "이",
    "가",
    "로",
    "으로",
    "와",
    "과",
    "도",
    "의",
    "해서",
    "했어",
    "하고",
    "좀",
    "같은",
    "이라서",
    "해줘",
    "했는데",
    "합니다",
    "에게",
];
/// What a single letter labels (n제, x축, A급).
const LABELED: [&str; 14] = [
    "제", "축", "급", "팀", "형", "번", "위", "점", "층", "조", "호", "판", "기", "군",
];

/// One token and what it should become; `None` means it stays a single-language reading.
struct Token {
    keys: String,
    expected: Option<String>,
}

struct Set {
    name: &'static str,
    weight: f64,
    train: Vec<Token>,
    held_out: Vec<Token>,
}

impl Set {
    fn new(name: &'static str, weight: f64, tokens: Vec<Token>) -> Self {
        let (mut train, mut held_out) = (Vec::new(), Vec::new());
        for (i, token) in tokens.into_iter().enumerate() {
            if i % 5 == 0 {
                held_out.push(token);
            } else {
                train.push(token);
            }
        }
        Self {
            name,
            weight,
            train,
            held_out,
        }
    }
}

fn has_hangul(s: &str) -> bool {
    s.chars().any(|c| ('가'..='힣').contains(&c))
}

fn has_latin(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_alphabetic())
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

/// Tokens of every hand-written case that the search sees: the expected texts of the
/// code-switch, word and sentence lists, cut at spaces and symbols (http://x → http, x),
/// keeping the ones whose keys are letters only.
fn hand_tokens() -> Vec<Token> {
    let texts = data_lines(CODE_SWITCH)
        .map(|f| f[0])
        .chain(data_lines(WORDS).map(|f| f[2]))
        .chain(data_lines(SENTENCES).map(|f| f[1]));
    let mut out = Vec::new();
    for text in texts {
        for word in text.split(|c: char| !(c.is_alphanumeric() || ('가'..='힣').contains(&c))) {
            let keys = keys_of_text(word);
            if keys.len() < 2 || !keys.bytes().all(|b| b.is_ascii_alphabetic()) {
                continue;
            }
            let mixed = has_hangul(word) && has_latin(word);
            out.push(Token {
                keys,
                expected: mixed.then(|| word.to_string()),
            });
        }
    }
    out
}

/// English word + Korean ending, a capitalised one now and then, and a letter + a labelled noun.
fn synthetic_tokens() -> Vec<Token> {
    let vocab = korean_list(KOREAN_VOCAB);
    let mut out = Vec::new();
    for (i, word) in english_list()
        .into_iter()
        .filter(|w| w.len() >= 3)
        .enumerate()
    {
        let korean = if i % 3 == 0 {
            vocab[(i / 3) % vocab.len()]
        } else {
            ENDINGS[i % ENDINGS.len()]
        };
        let english = if i % 4 == 0 {
            capitalized(word)
        } else {
            word.to_string()
        };
        let text = format!("{english}{korean}");
        out.push(Token {
            keys: keys_of_text(&text),
            expected: Some(text),
        });
    }
    for (i, noun) in LABELED.iter().enumerate() {
        for letter in ('a'..='z').step_by(3).chain(['A', 'B', 'X']) {
            let letter = if i % 2 == 0 {
                letter
            } else {
                letter.to_ascii_uppercase()
            };
            let text = format!("{letter}{noun}");
            out.push(Token {
                keys: keys_of_text(&text),
                expected: Some(text),
            });
        }
    }
    out
}

fn terms() -> Vec<&'static str> {
    TERMS
        .lines()
        .filter(|l| l.len() >= 3 && l.bytes().all(|b| b.is_ascii_alphabetic()))
        .collect()
}

/// The endings the learning simulation in lab/src/corpus.rs puts after a term.
const LEARN_ENDINGS: [&str; 8] = ["이", "을", "에", "에서", "으로", "는", "와", "도"];

/// A term with a Korean ending straight after it: every ending of the learning simulation.
fn term_tokens() -> Vec<Token> {
    let mut out = Vec::new();
    for term in terms() {
        for ending in LEARN_ENDINGS {
            let text = format!("{term}{ending}");
            out.push(Token {
                keys: keys_of_text(&text),
                expected: Some(text),
            });
        }
    }
    out
}

/// A term with a Korean word of the vocabulary straight after it: kubectl확인, docker설치.
fn term_word_tokens() -> Vec<Token> {
    let vocab = korean_list(KOREAN_VOCAB);
    let mut out = Vec::new();
    for (i, term) in terms().into_iter().enumerate() {
        for k in 0..3 {
            let text = format!("{term}{}", vocab[(i * 7 + k * 131) % vocab.len()]);
            out.push(Token {
                keys: keys_of_text(&text),
                expected: Some(text),
            });
        }
    }
    out
}

/// The terms alone: English, no cut.
fn bare_terms() -> Vec<Token> {
    terms()
        .into_iter()
        .map(|t| Token {
            keys: t.to_string(),
            expected: None,
        })
        .collect()
}

/// Distinct whitespace tokens of a corpus file, spread evenly, as words that must stay uncut:
/// real sentences hold glued, misspelt and rare words that no word list names.
fn corpus_tokens(file: &str, hangul: bool, limit: usize) -> Vec<Token> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join(file);
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let words: Vec<&str> = text
        .lines()
        .flat_map(str::split_whitespace)
        .filter(|w| {
            if hangul {
                w.chars().count() >= 2 && w.chars().all(|c| ('가'..='힣').contains(&c))
            } else {
                w.len() >= 2 && w.bytes().all(|b| b.is_ascii_alphabetic())
            }
        })
        .filter(|w| seen.insert(*w))
        .collect();
    let step = (words.len() / limit).max(1);
    words
        .into_iter()
        .step_by(step)
        .take(limit)
        .map(|w| Token {
            keys: if hangul {
                keys_of_text(w)
            } else {
                w.to_string()
            },
            expected: None,
        })
        .collect()
}

/// Rare English words, spread over the whole list.
fn web2_words(limit: usize) -> Vec<Token> {
    let words: Vec<&str> = WEB2
        .lines()
        .filter(|w| w.len() >= 3 && w.bytes().all(|b| b.is_ascii_lowercase()))
        .collect();
    let step = (words.len() / limit).max(1);
    words
        .into_iter()
        .step_by(step)
        .take(limit)
        .map(|w| Token {
            keys: w.to_string(),
            expected: None,
        })
        .collect()
}

fn english_words() -> Vec<Token> {
    english_list()
        .into_iter()
        .map(|w| Token {
            keys: w.to_string(),
            expected: None,
        })
        .collect()
}

fn korean_words() -> Vec<Token> {
    let mut words = korean_list(KOREAN_WORDS);
    words.extend(korean_list(KOREAN_VOCAB));
    words
        .into_iter()
        .map(|w| Token {
            keys: keys_of_text(w),
            expected: None,
        })
        .collect()
}

fn correct(cs: &CodeSwitch, token: &Token) -> bool {
    let decision = cs.decide(&token.keys, &DecisionContext::default());
    match (&token.expected, decision) {
        (Some(want), Some(got)) => &got.output == want,
        (None, None) => true,
        _ => false,
    }
}

fn accuracy(cs: &CodeSwitch, tokens: &[Token]) -> f64 {
    if tokens.is_empty() {
        return 1.0;
    }
    tokens.iter().filter(|t| correct(cs, t)).count() as f64 / tokens.len() as f64
}

/// A correct cut counts in full only when it wins by `SAFE` more than the margin asks: a
/// narrow win flips with the next small change of the weights.
const SAFE: f64 = 1.5;

fn credit(cs: &CodeSwitch, token: &Token) -> f64 {
    if !correct(cs, token) {
        return 0.0;
    }
    let narrow = token.expected.is_some()
        && cs
            .analyze(&token.keys, &DecisionContext::default())
            .is_some_and(|s| s.gap < cs.margin() + SAFE);
    if narrow {
        0.5
    } else {
        1.0
    }
}

/// Weighted mean credit over the training halves.
fn objective(cs: &CodeSwitch, sets: &[Set]) -> f64 {
    let total: f64 = sets.iter().map(|s| s.weight).sum();
    sets.iter()
        .map(|s| {
            let sum: f64 = s.train.iter().map(|t| credit(cs, t)).sum();
            s.weight * sum / s.train.len().max(1) as f64
        })
        .sum::<f64>()
        / total
}

type Field = (&'static str, fn(&mut Params) -> &mut f64, &'static [f64]);

const FIELDS: [Field; 11] = [
    (
        "lead_ko",
        |p| &mut p.lead_ko,
        &[0.0, -2.0, -4.0, -6.0, -8.0],
    ),
    (
        "en_bias",
        |p| &mut p.en_bias,
        &[-1.5, -1.0, -0.5, 0.0, 0.5, 1.0],
    ),
    (
        "whole_bonus",
        |p| &mut p.whole_bonus,
        &[0.0, 2.0, 4.0, 6.0, 8.0, 12.0],
    ),
    (
        "switch_cost",
        |p| &mut p.switch_cost,
        &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0],
    ),
    (
        "dict_bonus",
        |p| &mut p.dict_bonus,
        &[0.0, 0.5, 1.0, 1.5, 2.0, 3.0],
    ),
    (
        "ending_bonus",
        |p| &mut p.ending_bonus,
        &[0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0],
    ),
    (
        "label_logp",
        |p| &mut p.label_logp,
        &[-2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -10.0],
    ),
    (
        "word_bonus",
        |p| &mut p.word_bonus,
        &[0.0, 2.0, 4.0, 6.0, 8.0, 12.0, 16.0],
    ),
    (
        "cap_penalty",
        |p| &mut p.cap_penalty,
        &[0.0, 3.0, 6.0, 10.0],
    ),
    (
        "bare_tail",
        |p| &mut p.bare_tail,
        &[0.0, -2.0, -4.0, -6.0, -8.0],
    ),
    (
        "margin",
        |p| &mut p.margin,
        &[0.0, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0],
    ),
];

fn report(cs: &CodeSwitch, sets: &[Set]) {
    for set in sets {
        println!(
            "  {:10} train {:6.2}%  held-out {:6.2}%  (n={}+{})",
            set.name,
            accuracy(cs, &set.train) * 100.0,
            accuracy(cs, &set.held_out) * 100.0,
            set.train.len(),
            set.held_out.len()
        );
    }
}

/// Wrongly cut words, and the narrowest correct cuts, with their gaps: shows whether the score
/// separates them at all.
fn show_gaps(cs: &CodeSwitch, sets: &[Set]) {
    let context = DecisionContext::default();
    let gap = |keys: &str| cs.analyze(keys, &context).map(|s| (s.gap, s.explanation()));
    for set in sets.iter().filter(|s| {
        matches!(
            s.name,
            "english" | "korean" | "bare terms" | "ko tokens" | "en tokens" | "web2"
        )
    }) {
        let mut wrong: Vec<(f64, String)> = set
            .train
            .iter()
            .chain(&set.held_out)
            .filter(|t| !correct(cs, t))
            .filter_map(|t| gap(&t.keys).map(|(g, e)| (g, format!("{} {e}", t.keys))))
            .collect();
        wrong.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("wrongly cut {} ({}):", set.name, wrong.len());
        for (_, line) in wrong.iter().take(12) {
            println!("    {line}");
        }
    }
    for set in sets
        .iter()
        .filter(|s| s.name == "hand" || s.name == "synthetic" || s.name == "terms" || s.name == "term+word")
    {
        let missed: Vec<String> = set
            .train
            .iter()
            .chain(&set.held_out)
            .filter(|t| t.expected.is_some() && !correct(cs, t))
            .map(|t| {
                let got = cs.decide(&t.keys, &context).map(|d| d.output);
                let (gap, cut) = gap(&t.keys).unwrap_or((f64::NAN, String::new()));
                format!(
                    "{} → {} (want {}) best cut {cut} gap {gap:+.1}",
                    t.keys,
                    got.as_deref().unwrap_or("-"),
                    t.expected.as_deref().unwrap_or("")
                )
            })
            .collect();
        println!("missed {} ({}):", set.name, missed.len());
        for line in missed.iter().take(14) {
            println!("    {line}");
        }
        let mut right: Vec<(f64, String)> = set
            .train
            .iter()
            .chain(&set.held_out)
            .filter(|t| t.expected.is_some() && correct(cs, t))
            .filter_map(|t| gap(&t.keys).map(|(g, e)| (g, format!("{} {e}", t.keys))))
            .collect();
        right.sort_by(|a, b| a.0.total_cmp(&b.0));
        println!("narrowest correct {} ({}):", set.name, right.len());
        for (_, line) in right.iter().take(10) {
            println!("    {line}");
        }
    }
}

pub fn run() {
    let sets = [
        Set::new("hand", 5.0, hand_tokens()),
        Set::new("synthetic", 1.0, synthetic_tokens()),
        Set::new("terms", 5.0, term_tokens()),
        Set::new("term+word", 5.0, term_word_tokens()),
        Set::new("bare terms", 20.0, bare_terms()),
        // A wrongly cut English or Korean word is much worse than a missed cut.
        Set::new("english", 20.0, english_words()),
        Set::new("korean", 20.0, korean_words()),
        Set::new("web2", 20.0, web2_words(12000)),
        Set::new(
            "ko tokens",
            20.0,
            corpus_tokens("ko_sentences.txt", true, 8000),
        ),
        Set::new(
            "en tokens",
            20.0,
            corpus_tokens("en_sentences.txt", false, 12000),
        ),
    ];
    let english = EnglishModel::shared();
    let mut params = Params::default();
    let start = std::time::Instant::now();
    println!(
        "start: objective {:.4}",
        objective(&CodeSwitch::new(english, params), &sets)
    );
    report(&CodeSwitch::new(english, params), &sets);
    if std::env::args().any(|a| a == "--gaps") {
        show_gaps(&CodeSwitch::new(english, params), &sets);
        return;
    }
    for sweep in 1..=3 {
        let before = params;
        for (name, field, grid) in FIELDS {
            let mut best = (
                objective(&CodeSwitch::new(english, params), &sets),
                *field(&mut params),
            );
            for &value in grid {
                let mut trial = params;
                *field(&mut trial) = value;
                let score = objective(&CodeSwitch::new(english, trial), &sets);
                if score > best.0 + 1e-9 {
                    best = (score, value);
                }
            }
            *field(&mut params) = best.1;
            println!(
                "sweep {sweep} {name:12} = {:6.2}  objective {:.4}",
                best.1, best.0
            );
        }
        if params == before {
            break;
        }
    }
    println!(
        "\nfitted in {:.0}s:\n{params:#?}",
        start.elapsed().as_secs_f64()
    );
    report(&CodeSwitch::new(english, params), &sets);
    show_gaps(&CodeSwitch::new(english, params), &sets);
}
