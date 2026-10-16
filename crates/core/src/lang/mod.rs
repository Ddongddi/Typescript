//! Language models: how Hangul is typed and composed, and how likely a token is Korean or English.

mod combo;
mod en;
pub mod english;
pub(crate) mod fnv;
pub mod hangul;
mod ja;
pub mod japanese;
mod ko;
pub mod korean;
pub mod module;
pub mod pack;
pub mod particles;
pub mod registry;
pub mod romaji;
pub mod set;
pub(crate) mod sorted;
mod syllables;
