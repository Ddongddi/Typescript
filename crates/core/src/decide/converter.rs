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
        let (mut after_digit, mut last) = (false, ' ');

        let flush = |token: &mut String,
                     output: &mut String,
                     words: &mut Vec<Word>,
                     previous: &mut Option<_>,
                     after_digit: bool| {
            if token.is_empty() {
                return;
            }
            let ctx = DecisionContext {
                previous_language: *previous,
                partial: false,
                after_digit,
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
                if token.is_empty() {
                    after_digit = last.is_ascii_digit();
                }
                token.push(ch);
            } else {
                flush(
                    &mut token,
                    &mut output,
                    &mut words,
                    &mut previous,
                    after_digit,
                );
                output.push(ch);
                if ch == '\n' || ch == '\r' {
                    previous = None;
                }
            }
            last = ch;
        }
        flush(
            &mut token,
            &mut output,
            &mut words,
            &mut previous,
            after_digit,
        );
        (output, words)
    }
}
