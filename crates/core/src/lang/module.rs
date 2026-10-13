//! What a language module provides. The engine and the session ask these instead of knowing
//! languages by name.

use crate::decide::engine::Language;

pub trait LanguageModule: Send + Sync {
    fn language(&self) -> Language;
    /// The text these keys type in this language; None when they can't (no Hangul for "ra-men").
    fn compose(&self, keys: &str) -> Option<String>;
    /// The reading has no stray pieces: Esc skips unclean readings while clean ones exist.
    fn is_clean(&self, keys: &str) -> bool;
}

/// A reading that exists only when two languages are on (English word + Korean particle).
pub trait ComboRule: Send + Sync {
    fn language(&self) -> Language;
    fn needs(&self) -> [Language; 2];
    fn compose(&self, keys: &str) -> Option<String>;
}
