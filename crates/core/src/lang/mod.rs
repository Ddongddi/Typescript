//! Language models: how Hangul is typed and composed, and how likely a token is Korean or English.

pub mod english;
pub mod hangul;
pub mod japanese;
pub mod korean;
pub mod particles;
pub mod romaji;
pub(crate) mod sorted;
mod syllables;
