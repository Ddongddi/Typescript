//! The kind of token a key string is, shared by many words: what an Esc correction teaches.
//! Correcting `ss` to ㄴㄴ says something about consonant-only keys, not only about `ss`.

use crate::decide::mixed::MixedWord;
use crate::lang::hangul::{is_consonant, HangulComposer, Unit};

/// Consonant-only keys read as chat jamo up to this length (ㅁㅊ, ㅈㅇ, ㅇㅈㅇ).
const JAMO_MAX_KEYS: usize = 3;
/// An English stem this long before a particle: nodejs + 의.
const STEM_MIN_KEYS: usize = 3;
const MEDIUM_FROM: usize = 4;
const LONG_FROM: usize = 7;

/// A short name for the kind of `keys`, in the order the checks run.
pub fn shape_of(keys: &str) -> &'static str {
    let len = keys.chars().count();
    if len <= JAMO_MAX_KEYS && is_only_consonants(keys) {
        return "jamo";
    }
    if keys.chars().any(char::is_uppercase) {
        return "capital";
    }
    if MixedWord::split(keys).is_some_and(|m| m.english.chars().count() >= STEM_MIN_KEYS) {
        return "ending";
    }
    match len {
        ..MEDIUM_FROM => "short",
        MEDIUM_FROM..LONG_FROM => "medium",
        _ => "long",
    }
}

fn is_only_consonants(keys: &str) -> bool {
    HangulComposer::compose(keys).is_some_and(|c| {
        !c.units.is_empty()
            && c.units
                .iter()
                .all(|u| matches!(u, Unit::Jamo(j) if is_consonant(*j)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_kind_of_token() {
        for (keys, shape) in [
            ("ss", "jamo"),
            ("ac", "jamo"),
            ("Eh", "capital"),
            ("nodejsdml", "ending"),
            ("so", "short"),
            ("thfxl", "medium"),
            ("dkssudgktpdy", "long"),
        ] {
            assert_eq!(shape_of(keys), shape, "{keys}");
        }
    }
}
