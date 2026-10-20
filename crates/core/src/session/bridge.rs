//! Platform-neutral input bridge: keys and optional text before the caret in, edit commands out.
//!
//! The commands are one string: `1` if the key was consumed (else `0`), then each command
//! preceded by `\u{1}`: `M<text>` set the composing text, `I<text>` commit it (replacing the
//! composing text), `B<n>` delete n UTF-16 units before the caret.

use super::testing::FakeTextField;
use super::{InputSession, Key, NoopClient, Range, TextClient};

const SEP: char = '\u{1}';

/// Key kinds shared by JNI and the C interface.
pub const LETTER: i32 = 0;
pub const PRINTABLE: i32 = 1;
pub const DELETE: i32 = 2;
pub const ESCAPE: i32 = 3;
pub const CYCLE: i32 = 4;
pub const ENTER: i32 = 5;
pub const TAB: i32 = 6;

fn key_for(kind: i32, ch: char) -> Key {
    match kind {
        LETTER => Key::Letter(ch),
        PRINTABLE => Key::Printable(ch),
        DELETE => Key::Delete,
        ESCAPE => Key::Escape,
        CYCLE => Key::Cycle,
        ENTER => Key::Enter,
        TAB => Key::Tab,
        _ => Key::Other,
    }
}

/// A mirror of the text before the caret that writes down what the session does to it.
struct Recorder {
    field: FakeTextField,
    coordinate_origin: Option<usize>,
    ops: Vec<String>,
}

impl Recorder {
    /// `before`: the editor's text before the caret, `composing` (a suffix of it) underlined.
    fn new(before: &str, composing: &str, coordinate_origin: Option<usize>) -> Self {
        let units: Vec<u16> = before.encode_utf16().collect();
        let plain = units
            .len()
            .checked_sub(composing.encode_utf16().count())
            .expect("the composition is a suffix of the mirrored text");
        let mut field = FakeTextField::new();
        field.insert_text(&String::from_utf16_lossy(&units[..plain]), None);
        if !composing.is_empty() {
            field.set_marked_text(composing);
        }
        Self {
            field,
            coordinate_origin,
            ops: Vec::new(),
        }
    }

    fn to_external(&self, range: Range) -> Range {
        if range.location == super::NOT_FOUND {
            return range;
        }
        let Some(location) = self
            .coordinate_origin
            .and_then(|origin| origin.checked_add(range.location))
        else {
            return Range {
                location: super::NOT_FOUND,
                length: 0,
            };
        };
        if location == super::NOT_FOUND || location.checked_add(range.length).is_none() {
            return Range {
                location: super::NOT_FOUND,
                length: 0,
            };
        }
        Range {
            location,
            length: range.length,
        }
    }

    fn to_local(&self, range: Range) -> Option<Range> {
        if range.location == super::NOT_FOUND {
            return None;
        }
        let origin = self.coordinate_origin?;
        range.location.checked_add(range.length)?;
        let location = range.location.checked_sub(origin)?;
        location.checked_add(range.length)?;
        let local = Range {
            location,
            length: range.length,
        };
        self.field.text_in(local)?;
        Some(local)
    }

    fn composing(&self) -> String {
        let marked = self.field.marked_range();
        self.field.text_in(marked).unwrap_or_default()
    }

    fn render(&self, consumed: bool) -> String {
        let mut out = String::from(if consumed { '1' } else { '0' });
        for op in &self.ops {
            out.push(SEP);
            out.push_str(op);
        }
        out
    }
}

impl TextClient for Recorder {
    fn set_marked_text(&mut self, text: &str) {
        self.field.set_marked_text(text);
        self.ops.push(format!("M{text}"));
    }

    fn insert_text(&mut self, text: &str, replacing: Option<Range>) {
        let mut text = text.to_string();
        let replacing = match replacing {
            Some(range) => {
                let Some(local) = self.to_local(range) else {
                    return;
                };
                let cursor = self.field.selected_range().location;
                let Some(end) = local.location.checked_add(local.length) else {
                    return;
                };
                let Some(deleted) = cursor.checked_sub(local.location) else {
                    return;
                };
                if end > cursor {
                    return;
                }
                // Android deletes relative to the caret; what lies between the range and the
                // caret is typed again after the new text.
                let tail = Range {
                    location: end,
                    length: cursor - end,
                };
                let Some(tail_text) = self.field.text_in(tail) else {
                    return;
                };
                text.push_str(&tail_text);
                self.ops.push(format!("B{deleted}"));
                Some(local)
            }
            None => None,
        };
        self.field.insert_text(&text, replacing);
        self.ops.push(format!("I{text}"));
    }

    fn selected_range(&self) -> Range {
        self.to_external(self.field.selected_range())
    }
    fn marked_range(&self) -> Range {
        self.to_external(self.field.marked_range())
    }
    fn text_in(&self, range: Range) -> Option<String> {
        self.field.text_in(self.to_local(range)?)
    }
}

pub struct Ime {
    session: InputSession,
    composing: String,
}

impl Ime {
    /// An input method over a session the caller built (the desktop text services).
    pub fn with_session(session: InputSession) -> Self {
        Self {
            session,
            composing: String::new(),
        }
    }

    /// A new text field: `package` picks the app's policy.
    pub fn activate(&mut self, package: &str) {
        self.reset();
        self.session.activate(Some(package));
    }

    fn run(
        &mut self,
        before: Option<&str>,
        before_start: Option<usize>,
        f: impl FnOnce(&mut InputSession, &mut Recorder) -> bool,
    ) -> String {
        let before = match before {
            Some(before) => {
                if !before.ends_with(&self.composing) {
                    // The caret moved away from the word: the app ended the composition.
                    self.reset();
                }
                before
            }
            None => &self.composing,
        };
        let mut rec = Recorder::new(before, &self.composing, before_start);
        let consumed = f(&mut self.session, &mut rec);
        // Without a coordinate identity, any commit made during this call must not leave an
        // undo window that could later be matched to an unrelated recovered document suffix.
        let valid_commit_range = self.session.last_commit.as_ref().is_none_or(|commit| {
            commit.location != super::NOT_FOUND
                && commit
                    .location
                    .checked_add(commit.text.encode_utf16().count())
                    .is_some_and(|end| end != super::NOT_FOUND)
        });
        if before_start.is_none() || !valid_commit_range {
            self.session.close_flip_window();
        }
        self.composing = rec.composing();
        rec.render(consumed)
    }

    /// `before_start` is the UTF-16 coordinate of `before`'s first unit in the caller's stable
    /// text-field coordinate domain. With no `before`, it is the owned composition's start;
    /// `None` means the coordinate identity is unknown. A later matching suffix does not restore
    /// the identity of a commit made while its origin was unknown. `Some(before)` is authoritative,
    /// including `Some("")`. When before is `None`, only this bridge's composition is retained,
    /// not external text or committed history.
    pub fn key(
        &mut self,
        kind: i32,
        ch: char,
        before: Option<&str>,
        before_start: Option<usize>,
    ) -> String {
        self.run(before, before_start, |s, c| s.handle(key_for(kind, ch), c))
    }

    /// Forgets the word in progress; the app already ended its composition or changed text-field
    /// coordinate domains (focus, document, or caret moved). Transiently unavailable text may use
    /// `None` without resetting while the same owned composition remains active.
    pub fn reset(&mut self) {
        self.session.commit_for_new_context(&mut NoopClient);
        self.composing.clear();
    }

    /// Pauses or resumes tacit using the same text and coordinate contract as `key`.
    pub fn toggle_pause(&mut self, before: Option<&str>, before_start: Option<usize>) -> String {
        self.run(before, before_start, |s, c| {
            s.toggle_pause(c);
            true
        })
    }

    pub fn paused(&self) -> bool {
        self.session.paused()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrepresentable_commit_coordinates_cannot_leave_a_flip_window() {
        for origin in [
            super::super::NOT_FOUND,
            super::super::NOT_FOUND - 1,
            super::super::NOT_FOUND - 2,
        ] {
            let languages = crate::Languages::legacy(false);
            let session = InputSession::new(
                crate::make_engine(None, &languages),
                crate::AppPolicies::default(),
                true,
            );
            let mut ime = Ime::with_session(session);
            ime.activate("test");
            assert!(ime.key(LETTER, 'd', None, Some(origin)).starts_with('1'));
            ime.key(PRINTABLE, '💡', None, Some(origin));
            assert!(!ime.session.has_flip_window());
        }
    }
}
