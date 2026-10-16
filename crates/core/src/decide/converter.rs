//! Converts a whole line of QWERTY text word by word, carrying language context across
//! words exactly as the input method does while typing (TextConverter.swift), then takes a
//! second look at the words it was unsure of.

use crate::decide::engine::{Decision, DecisionContext, DecisionEngine};

/// A word decided below this confidence is a coin flip (so, Eh, rid)…
const UNSURE_BELOW: f64 = 0.8;
/// …and a later word decided from this confidence says which language the text is in.
const SURE_FROM: f64 = 0.95;

pub struct Word {
    pub keys: String,
    pub decision: Decision,
}

pub struct TextConverter<'a> {
    pub engine: &'a dyn DecisionEngine,
    look_ahead: bool,
}

/// One alphabetic run of the text.
struct Token {
    start: usize,
    end: usize,
    after_digit: bool,
    /// A line break sits between this token and the one before.
    new_line: bool,
}

impl<'a> TextConverter<'a> {
    pub fn new(engine: &'a dyn DecisionEngine) -> Self {
        Self {
            engine,
            look_ahead: true,
        }
    }

    /// Left to right only, as the input method decides while typing.
    pub fn without_look_ahead(mut self) -> Self {
        self.look_ahead = false;
        self
    }

    pub fn convert(&self, text: &str) -> (String, Vec<Word>) {
        let tokens = tokens(text);
        let mut decisions: Vec<Decision> = Vec::with_capacity(tokens.len());
        let mut previous = None;
        for t in &tokens {
            if t.new_line {
                previous = None;
            }
            let ctx = DecisionContext {
                previous_language: previous,
                partial: false,
                after_digit: t.after_digit,
            };
            let decision = self.engine.decide(&text[t.start..t.end], &ctx);
            previous = Some(decision.language.sentence_language());
            decisions.push(decision);
        }
        if self.look_ahead {
            self.second_look(text, &tokens, &mut decisions);
        }
        let mut output = String::new();
        let mut words = Vec::with_capacity(tokens.len());
        let mut at = 0;
        for (t, decision) in tokens.iter().zip(decisions) {
            output.push_str(&text[at..t.start]);
            output.push_str(&decision.output);
            at = t.end;
            words.push(Word {
                keys: text[t.start..t.end].to_string(),
                decision,
            });
        }
        output.push_str(&text[at..]);
        (output, words)
    }

    /// A word that was a coin flip is decided again when a sure word of the other language
    /// follows on the same line, as if that language had come before it: `so` alone is English,
    /// but `so dlrltlarhk` is 내 이기심.
    fn second_look(&self, text: &str, tokens: &[Token], decisions: &mut [Decision]) {
        for i in 0..tokens.len() {
            if decisions[i].confidence >= UNSURE_BELOW {
                continue;
            }
            let sure = (i + 1..tokens.len())
                .take_while(|j| !tokens[*j].new_line)
                .find(|j| decisions[*j].confidence >= SURE_FROM);
            let Some(j) = sure else { continue };
            let wanted = decisions[j].language.sentence_language();
            if decisions[i].language.sentence_language() == wanted {
                continue;
            }
            let ctx = DecisionContext {
                previous_language: Some(wanted),
                partial: false,
                after_digit: tokens[i].after_digit,
            };
            decisions[i] = self
                .engine
                .decide(&text[tokens[i].start..tokens[i].end], &ctx);
        }
    }
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut open: Option<usize> = None;
    let (mut last, mut new_line, mut after_digit) = (' ', false, false);
    for (i, ch) in text.char_indices() {
        if ch.is_ascii_alphabetic() {
            if open.is_none() {
                open = Some(i);
                after_digit = last.is_ascii_digit();
            }
        } else {
            if let Some(start) = open.take() {
                out.push(Token {
                    start,
                    end: i,
                    after_digit,
                    new_line,
                });
                new_line = false;
            }
            if ch == '\n' || ch == '\r' {
                new_line = true;
            }
        }
        last = ch;
    }
    if let Some(start) = open {
        out.push(Token {
            start,
            end: text.len(),
            after_digit,
            new_line,
        });
    }
    out
}
