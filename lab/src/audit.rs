//! Candidate coverage with the real left-to-right converter's contexts, without future text.
use std::{collections::BTreeMap, sync::Mutex};

use tacit_core::{
    decide::codeswitch::{CodeSwitch, Kind},
    lang::hangul::HangulComposer,
    Decision, DecisionContext, DecisionEngine, EnglishModel, TextConverter,
};

struct Recording<'a> {
    engine: &'a dyn DecisionEngine,
    contexts: Mutex<Vec<DecisionContext>>,
}

impl DecisionEngine for Recording<'_> {
    fn decide(&self, keys: &str, context: &DecisionContext) -> Decision {
        self.contexts.lock().unwrap().push(*context);
        self.engine.decide(keys, context)
    }
}

/// Map physical key boundaries to gold UTF-8 boundaries; positions inside a syllable stay None.
fn alignment(text: &str) -> (String, Vec<Option<usize>>) {
    let mut keys = String::new();
    let mut at = vec![Some(0)];
    for (byte, c) in text.char_indices() {
        let typed = crate::keys_of_text(&c.to_string());
        keys.push_str(&typed);
        at.resize(keys.len() + 1, None);
        at[keys.len()] = Some(byte + c.len_utf8());
    }
    (keys, at)
}

fn permits(
    keys: &str,
    gold: &str,
    at: &[Option<usize>],
    from: usize,
    to: usize,
    kind: Kind,
) -> bool {
    let (Some(start), Some(end)) = (at[from], at[to]) else {
        return false;
    };
    let expected = &gold[start..end];
    match kind {
        Kind::English => keys[from..to] == *expected,
        Kind::Korean => {
            HangulComposer::compose(&keys[from..to]).is_some_and(|c| c.text() == expected)
        }
    }
}

pub fn sentences(engine: &dyn DecisionEngine, name: &str, texts: &[String], shown: usize) {
    let search = CodeSwitch::new(EnglishModel::shared(), Default::default());
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut examples = Vec::new();
    let (mut exact, mut offline_exact, mut tokens) = (0, 0, 0);
    for text in texts {
        let (keys, at) = alignment(text);
        offline_exact += usize::from(TextConverter::new(engine).convert(&keys).0 == *text);
        let recorder = Recording {
            engine,
            contexts: Mutex::new(Vec::new()),
        };
        let (output, words) = TextConverter::new(&recorder)
            .without_look_ahead()
            .convert(&keys);
        exact += usize::from(output == *text);
        let contexts = recorder.contexts.into_inner().unwrap();
        assert_eq!(words.len(), contexts.len());
        let mut cursor = 0;
        for (word, context) in words.iter().zip(contexts) {
            let from = cursor
                + keys[cursor..]
                    .find(&word.keys)
                    .expect("converter word in input");
            let to = from + word.keys.len();
            cursor = to;
            tokens += 1;
            let (Some(start), Some(end)) = (at[from], at[to]) else {
                *counts.entry("unaligned").or_default() += 1;
                continue;
            };
            let expected = &text[start..end];
            let category = if word.decision.output == expected {
                "correct"
            } else if word.keys == expected
                || HangulComposer::compose(&word.keys).is_some_and(|c| c.text() == expected)
            {
                // Raw English and whole-token Hangul are renderable without the mixed lattice.
                "pure-selection"
            } else if let Some(audit) =
                search.audit_candidates(&word.keys, &context, |a, b, kind| {
                    permits(&keys, text, &at, from + a, from + b, kind)
                })
            {
                if !audit.edges_reach_gold {
                    "missing-edge"
                } else if !audit.transitions_reach_gold {
                    "blocked-transition"
                } else if audit.skipped {
                    if audit.selected.as_deref() == Some(expected) {
                        "skip-gate"
                    } else {
                        "skip-and-selection"
                    }
                } else if audit.selected.as_deref() != Some(expected) {
                    "lattice-selection"
                } else {
                    "outside-lattice"
                }
            } else {
                "unsupported"
            };
            *counts.entry(category).or_default() += 1;
            if category != "correct" && examples.len() < shown {
                examples.push(format!(
                    "{category}: {} -> {} (want {expected}, previous={:?})",
                    word.keys, word.decision.output, context.previous_language
                ));
            }
        }
    }
    assert_eq!(counts.values().sum::<usize>(), tokens);
    println!("\n{name}: sentences={} offline-exact={offline_exact} left-to-right-exact={exact} tokens={tokens}", texts.len());
    for category in [
        "correct",
        "pure-selection",
        "missing-edge",
        "blocked-transition",
        "skip-gate",
        "skip-and-selection",
        "lattice-selection",
        "outside-lattice",
        "unaligned",
        "unsupported",
    ] {
        let count = counts.get(category).copied().unwrap_or(0);
        println!(
            "  {category:22} {count:8} {:8.3}%",
            100.0 * count as f64 / tokens.max(1) as f64
        );
    }
    for example in examples {
        println!("  {example}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_alignment_preserves_unicode_and_cannot_cut_a_syllable() {
        let gold = "x값에API";
        let (keys, at) = alignment(gold);
        assert_eq!(keys, "xrkqtdpAPI");
        assert!(permits(&keys, gold, &at, 0, 1, Kind::English));
        assert!(permits(&keys, gold, &at, 1, 5, Kind::Korean));
        assert!(!permits(&keys, gold, &at, 1, 4, Kind::Korean));
        assert!(!permits(&keys, gold, &at, 1, 5, Kind::English));
        assert_eq!(at[keys.len()], Some(gold.len()));
    }
}
