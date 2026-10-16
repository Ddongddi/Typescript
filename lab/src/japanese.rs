//! Japanese scenario set: romaji keys typed through the real session with the embedded
//! Japanese pack and the same kana→kanji converter the text service uses. Each sentence is
//! typed alone, converted with Space and committed with Return; the text the sentence should
//! end up as is the expected one. Misses are the work list, like the English/Korean sets.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use typescript_core::session::testing::FakeTextField;
use typescript_core::{
    make_engine, AppPolicies, DictionaryConverter, InputSession, JapaneseConverter, Key, Language,
    Languages, TextClient,
};

use crate::corpus::Report;
use crate::scenario::Tally;

const JAPANESE: &str = include_str!("../scenarios/japanese.tsv");

/// What the sentence should become, the romaji keys that type it, and its scenario tag.
fn items() -> Vec<(&'static str, &'static str, &'static str)> {
    JAPANESE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut fields = l.split('\t');
            match (fields.next(), fields.next(), fields.next()) {
                (Some(expected), Some(keys), Some(tag)) => (expected, keys, tag),
                _ => panic!("japanese scenario line without three tabs: {l:?}"),
            }
        })
        .collect()
}

pub fn run(shown: usize) -> Report {
    let mut report = Report {
        metrics: Vec::new(),
        misses: Vec::new(),
    };
    if !typescript_core::pack::available(Language::Japanese) {
        return report;
    }
    let engine = make_engine(None, &Languages::legacy(true));
    let converter: Arc<dyn JapaneseConverter> = Arc::new(DictionaryConverter::new(None));
    let mut tags: BTreeMap<&'static str, Tally> = BTreeMap::new();
    let mut misses = Vec::new();
    let (mut chars, mut elapsed) = (0usize, 0f64);
    for (expected, keys, tag) in items() {
        let mut session = InputSession::new(engine.clone(), AppPolicies::default(), false)
            .with_japanese(converter.clone(), false);
        let mut field = FakeTextField::new();
        let start = Instant::now();
        for ch in keys.chars() {
            let key = if ch.is_ascii_alphabetic() {
                Key::Letter(ch)
            } else {
                Key::Printable(ch)
            };
            if !session.handle(key, &mut field) {
                field.insert_text(&ch.to_string(), None);
            }
        }
        session.handle(Key::Printable(' '), &mut field);
        session.handle(Key::Enter, &mut field);
        elapsed += start.elapsed().as_secs_f64() * 1e6;
        chars += expected.chars().count();
        let got = field.text();
        tags.entry(tag).or_default().add(expected, &got);
        if got != expected {
            misses.push(format!("[{tag}] {keys} → {got} (want {expected})"));
        }
    }
    let mut all = Tally::default();
    for (tag, tally) in &tags {
        report
            .metrics
            .push((format!("ja.{tag}.sentence"), tally.sentence_pct()));
        all.merge(tally);
    }
    report
        .metrics
        .push(("ja.ALL.sentence".into(), all.sentence_pct()));
    report
        .metrics
        .push(("ja.ALL.us_per_char".into(), elapsed / chars.max(1) as f64));
    misses.truncate(shown.max(1) * 4);
    report.misses.push(("ja".to_string(), misses));
    report
}
