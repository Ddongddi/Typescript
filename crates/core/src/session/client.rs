//! The text field the input method types into, as the session sees it.

pub const NOT_FOUND: usize = usize::MAX;

/// A range in UTF-16 code units, like NSRange / ITfRangeACP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub location: usize,
    pub length: usize,
}

impl Range {
    pub fn end(&self) -> usize {
        self.location + self.length
    }
}

/// The text field the input method is typing into.
pub trait TextClient {
    fn set_marked_text(&mut self, text: &str);
    /// `replacing == None` replaces the marked text (or the selection if nothing is marked).
    fn insert_text(&mut self, text: &str, replacing: Option<Range>);
    fn selected_range(&self) -> Range;
    /// `location == NOT_FOUND` when nothing is composing.
    fn marked_range(&self) -> Range;
    fn text_in(&self, range: Range) -> Option<String>;
}

/// A client that takes nothing: for resetting state after the app already ended the composition.
pub struct NoopClient;

impl TextClient for NoopClient {
    fn set_marked_text(&mut self, _: &str) {}
    fn insert_text(&mut self, _: &str, _: Option<Range>) {}
    fn selected_range(&self) -> Range {
        Range {
            location: NOT_FOUND,
            length: 0,
        }
    }
    fn marked_range(&self) -> Range {
        Range {
            location: NOT_FOUND,
            length: 0,
        }
    }
    fn text_in(&self, _: Range) -> Option<String> {
        None
    }
}
