//! Corpus accuracy of the language decision: Korean words typed on a 2-set layout must come out
//! Korean, frequent English words English. Run with `--nocapture` to see the misses.

mod common;

use common::keys_of;
use typescript_core::{DecisionContext, DecisionEngine, HeuristicEngine, Language};

const KOREAN_WORDS: &str = include_str!("../data/korean_words.txt");
const KOREAN_VOCAB: &str = include_str!("../data/korean_vocabulary.txt");
const ENGLISH_FREQ: &str = include_str!("../data/english_frequency.txt");

fn korean_words(text: &str, min_syllables: usize) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.split('\t').next())
        .filter(|w| {
            w.chars().count() >= min_syllables && w.chars().all(|c| ('가'..='힣').contains(&c))
        })
        .map(String::from)
        .collect()
}

fn accuracy(label: &str, keys: &[String], expected: Language) -> f64 {
    let engine = HeuristicEngine::new();
    let misses: Vec<&String> = keys
        .iter()
        .filter(|k| engine.decide(k, &DecisionContext::default()).language != expected)
        .collect();
    let rate = 1.0 - misses.len() as f64 / keys.len() as f64;
    println!(
        "{label}: {:.1}% of {} (first misses: {:?})",
        rate * 100.0,
        keys.len(),
        &misses[..misses.len().min(15)]
    );
    rate
}

#[test]
fn keys_round_trip_through_the_composer() {
    for w in ["안녕하세요", "값이", "읽었다", "의사", "왜", "뷁"] {
        let composed = typescript_core::lang::hangul::HangulComposer::compose(&keys_of(w)).unwrap();
        assert_eq!(composed.text(), w);
    }
}

#[test]
fn korean_text_is_recognised() {
    let words: Vec<String> = korean_words(KOREAN_WORDS, 2)
        .iter()
        .take(3000)
        .map(|w| keys_of(w))
        .collect();
    let vocab: Vec<String> = korean_words(KOREAN_VOCAB, 2)
        .iter()
        .take(3000)
        .map(|w| keys_of(w))
        .collect();
    let (w, v) = (
        accuracy("korean words", &words, Language::Korean),
        accuracy("korean vocabulary", &vocab, Language::Korean),
    );
    assert!(w > 0.99 && v > 0.99, "{w} {v}");
}

#[test]
fn frequent_english_is_recognised() {
    let words: Vec<String> = ENGLISH_FREQ
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|w| w.len() >= 3 && w.chars().all(|c| c.is_ascii_lowercase()))
        .take(3000)
        .map(String::from)
        .collect();
    assert!(accuracy("english words", &words, Language::English) > 0.99);
}
