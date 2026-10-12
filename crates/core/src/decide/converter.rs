//! Converts a whole line of QWERTY text word by word, carrying language context across
//! words exactly as the input method does while typing (TextConverter.swift).

use crate::decide::engine::{Decision, DecisionContext, DecisionEngine};

pub struct Word {
    pub keys: String,
    pub decision: Decision,
}

pub struct TextConverter<'a> {
    pub engine: &'a dyn DecisionEngine,
}

impl<'a> TextConverter<'a> {
    pub fn new(engine: &'a dyn DecisionEngine) -> Self {
        Self { engine }
    }

    pub fn convert(&self, text: &str) -> (String, Vec<Word>) {
        let mut output = String::new();
        let mut words = Vec::new();
        let mut token = String::new();
        let mut previous = None;

        let flush = |token: &mut String,
                     output: &mut String,
                     words: &mut Vec<Word>,
                     previous: &mut Option<_>| {
            if token.is_empty() {
                return;
            }
            let ctx = DecisionContext {
                previous_language: *previous,
                partial: false,
            };
            let decision = self.engine.decide(token, &ctx);
            output.push_str(&decision.output);
            *previous = Some(decision.language.sentence_language());
            words.push(Word {
                keys: std::mem::take(token),
                decision,
            });
        };

        for ch in text.chars() {
            if ch.is_ascii_alphabetic() {
                token.push(ch);
            } else {
                flush(&mut token, &mut output, &mut words, &mut previous);
                output.push(ch);
                if ch == '\n' || ch == '\r' {
                    previous = None;
                }
            }
        }
        flush(&mut token, &mut output, &mut words, &mut previous);
        (output, words)
    }
}
