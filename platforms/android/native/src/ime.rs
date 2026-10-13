//! The input method without Android: keys and the text before the caret in, edit commands out.
//!
//! The commands are one string: `1` if the key was consumed (else `0`), then each command
//! preceded by `\u{1}`: `M<text>` set the composing text, `I<text>` commit it (replacing the
//! composing text), `B<n>` delete n UTF-16 units before the caret.

use std::path::Path;
use std::sync::Arc;

use typescript_core::session::testing::FakeTextField;
use typescript_core::session::NoopClient;
use typescript_core::{
    make_engine, AppPolicies, InputSession, Key, LearningEngine, Range, Settings, TextClient,
};

const SEP: char = '\u{1}';

/// Key kinds the Java side sends.
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
    ops: Vec<String>,
}

impl Recorder {
    /// `before`: the editor's text before the caret, `composing` (a suffix of it) underlined.
    fn new(before: &str, composing: &str) -> Self {
        let units: Vec<u16> = before.encode_utf16().collect();
        let plain = units.len().saturating_sub(composing.encode_utf16().count());
        let mut field = FakeTextField::new();
        field.insert_text(&String::from_utf16_lossy(&units[..plain]), None);
        if !composing.is_empty() {
            field.set_marked_text(composing);
        }
        Self {
            field,
            ops: Vec::new(),
        }
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
        if let Some(range) = replacing {
            // Android deletes relative to the caret; what lies between the range and the caret
            // is typed again after the new text.
            let cursor = self.field.selected_range().location;
            let tail = Range {
                location: range.end().min(cursor),
                length: cursor.saturating_sub(range.end()),
            };
            text.push_str(&self.field.text_in(tail).unwrap_or_default());
            self.ops
                .push(format!("B{}", cursor.saturating_sub(range.location)));
        }
        self.field.insert_text(&text, replacing);
        self.ops.push(format!("I{text}"));
    }

    fn selected_range(&self) -> Range {
        self.field.selected_range()
    }
    fn marked_range(&self) -> Range {
        self.field.marked_range()
    }
    fn text_in(&self, range: Range) -> Option<String> {
        self.field.text_in(range)
    }
}

pub struct Ime {
    session: InputSession,
    composing: String,
}

impl Ime {
    /// `dir`: where the app keeps settings.json, apps.json, lexicon.json.
    pub fn new(dir: &Path) -> Self {
        let engine: Arc<LearningEngine> = make_engine(Some(dir.join("lexicon.json")), false);
        let settings = Settings::load(Some(&dir.join("settings.json")));
        let policies = AppPolicies::load(Some(&dir.join("apps.json")));
        // No word history and no Tab completion: the phone's keyboards have their own suggestions.
        let session = InputSession::new(engine, policies, settings.return_confirms_only);
        Self {
            session,
            composing: String::new(),
        }
    }

    /// A new text field: `package` picks the app's policy.
    pub fn activate(&mut self, package: &str) {
        self.session.activate(Some(package));
        self.composing.clear();
    }

    fn run(
        &mut self,
        before: &str,
        f: impl FnOnce(&mut InputSession, &mut Recorder) -> bool,
    ) -> String {
        if !before.ends_with(&self.composing) {
            self.reset(); // the caret moved away from the word: the app ended the composition
        }
        let mut rec = Recorder::new(before, &self.composing);
        let consumed = f(&mut self.session, &mut rec);
        self.composing = rec.composing();
        rec.render(consumed)
    }

    pub fn key(&mut self, kind: i32, ch: char, before: &str) -> String {
        self.run(before, |s, c| s.handle(key_for(kind, ch), c))
    }

    /// Forgets the word in progress; the app already ended its composition (focus or caret moved).
    pub fn reset(&mut self) {
        self.session.commit_for_new_context(&mut NoopClient);
        self.composing.clear();
    }

    /// Pauses or resumes Typescript in this text field.
    pub fn toggle_pause(&mut self, before: &str) -> String {
        self.run(before, |s, c| {
            s.toggle_pause(c);
            true
        })
    }

    pub fn paused(&self) -> bool {
        self.session.paused()
    }
}
