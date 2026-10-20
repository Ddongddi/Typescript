//! Drives `Ime` the way the Java side does: applies the returned commands to a text field and
//! types the keys the input method did not consume.

use tacit_android::ime::{Ime, DELETE, LETTER, PRINTABLE};

struct Editor {
    ime: Ime,
    /// Text before the caret (BMP only here, so chars are UTF-16 units).
    text: Vec<char>,
    composing: usize,
}

impl Editor {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ts-android-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut ime = tacit_android::new_ime(&dir);
        ime.activate("com.example.notes");
        Self {
            ime,
            text: Vec::new(),
            composing: 0,
        }
    }

    fn before(&self) -> String {
        self.text.iter().collect()
    }

    fn apply(&mut self, reply: &str) -> bool {
        let mut parts = reply.split('\u{1}');
        let consumed = parts.next() == Some("1");
        for op in parts {
            let (tag, rest) = op.split_at(1);
            match tag {
                "M" | "I" => {
                    self.text.truncate(self.text.len() - self.composing);
                    self.text.extend(rest.chars());
                    self.composing = if tag == "M" { rest.chars().count() } else { 0 };
                }
                "B" => {
                    let n: usize = rest.parse().unwrap();
                    self.text.truncate(self.text.len() - n);
                }
                other => panic!("unknown command {other}"),
            }
        }
        consumed
    }

    fn press(&mut self, kind: i32, ch: char) {
        let before = self.before();
        let reply = self.ime.key(kind, ch, Some(&before), Some(0));
        if !self.apply(&reply) {
            match kind {
                PRINTABLE => self.text.push(ch),
                DELETE => {
                    self.text.pop();
                }
                _ => {}
            }
        }
    }

    fn type_str(&mut self, s: &str) {
        for ch in s.chars() {
            let kind = if ch.is_ascii_alphabetic() {
                LETTER
            } else {
                PRINTABLE
            };
            self.press(kind, ch);
        }
    }

    fn shown(&self) -> String {
        self.before()
    }
}

#[test]
fn korean_keys_become_hangul_when_the_word_ends() {
    let mut e = Editor::new("ko");
    e.type_str("dkssud ");
    assert_eq!(e.shown(), "안녕 ");
}

#[test]
fn english_keys_stay_english() {
    let mut e = Editor::new("en");
    e.type_str("hello world ");
    assert_eq!(e.shown(), "hello world ");
}

#[test]
fn the_word_being_typed_is_underlined_as_it_grows() {
    let mut e = Editor::new("live");
    e.type_str("dkss");
    assert_eq!(e.composing, e.shown().chars().count());
    assert!(!e.shown().is_empty());
}

#[test]
fn delete_inside_a_word_edits_the_composition() {
    let mut e = Editor::new("del");
    e.type_str("hellp");
    e.press(DELETE, '\0');
    e.type_str("o ");
    assert_eq!(e.shown(), "hello ");
}

#[test]
fn text_already_in_the_field_is_left_alone() {
    let mut e = Editor::new("keep");
    e.text.extend("메모: ".chars());
    e.type_str("dkssud ");
    assert_eq!(e.shown(), "메모: 안녕 ");
}

#[test]
fn a_caret_moved_away_from_the_word_starts_a_new_one() {
    let mut e = Editor::new("moved");
    e.type_str("dkss");
    // The user taps elsewhere: the app commits the underlined text and the caret is elsewhere.
    e.composing = 0;
    e.text = "다른 글 ".chars().collect();
    e.type_str("hello ");
    assert_eq!(e.shown(), "다른 글 hello ");
}
