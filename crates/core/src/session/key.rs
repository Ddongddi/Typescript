//! The keys the session reacts to; the TIP maps virtual keys onto these.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Letter(char),
    Delete,
    Escape,
    /// Shift+Esc: switch the word's language in every app, including those where Esc goes to the app.
    Cycle,
    /// Space, digit or punctuation.
    Printable(char),
    Enter,
    /// Completes the word being typed, if there is a completion.
    Tab,
    /// Arrows, shortcuts…: commit and let the app handle the key.
    Other,
}
