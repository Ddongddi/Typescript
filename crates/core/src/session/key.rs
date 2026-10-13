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
    /// Shift+Space: a space of the other width (full-width where Space types a half-width one).
    ShiftSpace,
    Enter,
    /// Completes the word being typed, if there is a completion.
    Tab,
    /// ↓ and ↑: while converting, the next and previous candidate.
    Down,
    Up,
    /// ←/→: while converting, shorten or lengthen the first clause; otherwise like `other`.
    Left,
    Right,
    /// ⌃K / F7: the Japanese reading as katakana.
    Katakana,
    /// ⌃J / F6: the Japanese reading as hiragana.
    Hiragana,
    /// Arrows, shortcuts…: commit and let the app handle the key.
    Other,
}
