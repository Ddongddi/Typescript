//! A fake text field for tests and the CLI: UTF-16 offsets like TSF and NSRange, so the
//! session's range arithmetic is exercised the way a real application would.

use crate::session::{Range, TextClient, NOT_FOUND};

pub struct FakeTextField {
    text: Vec<u16>,
    marked: Range,
    /// The caret, UTF-16 units; tests move it to simulate a click elsewhere.
    pub cursor: usize,
    /// Simulates apps that don't support reading back their text.
    pub readable: bool,
}

impl Default for FakeTextField {
    fn default() -> Self {
        Self {
            text: Vec::new(),
            marked: Range {
                location: NOT_FOUND,
                length: 0,
            },
            cursor: 0,
            readable: true,
        }
    }
}

impl FakeTextField {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    fn replace(&mut self, target: Range, s: &str) -> usize {
        let new: Vec<u16> = s.encode_utf16().collect();
        let len = new.len();
        let end = (target.location + target.length).min(self.text.len());
        let start = target.location.min(end);
        self.text.splice(start..end, new);
        len
    }
}

impl TextClient for FakeTextField {
    fn set_marked_text(&mut self, text: &str) {
        let target = if self.marked.location != NOT_FOUND {
            self.marked
        } else {
            Range {
                location: self.cursor,
                length: 0,
            }
        };
        let len = self.replace(target, text);
        self.marked = if len > 0 {
            Range {
                location: target.location,
                length: len,
            }
        } else {
            Range {
                location: NOT_FOUND,
                length: 0,
            }
        };
        self.cursor = target.location + len;
    }

    fn insert_text(&mut self, text: &str, replacing: Option<Range>) {
        let target = replacing.unwrap_or(if self.marked.location != NOT_FOUND {
            self.marked
        } else {
            Range {
                location: self.cursor,
                length: 0,
            }
        });
        let len = self.replace(target, text);
        self.marked = Range {
            location: NOT_FOUND,
            length: 0,
        };
        self.cursor = target.location + len;
    }

    fn selected_range(&self) -> Range {
        Range {
            location: self.cursor,
            length: 0,
        }
    }

    fn marked_range(&self) -> Range {
        self.marked
    }

    fn text_in(&self, range: Range) -> Option<String> {
        if !self.readable || range.location + range.length > self.text.len() {
            return None;
        }
        Some(String::from_utf16_lossy(
            &self.text[range.location..range.location + range.length],
        ))
    }
}
