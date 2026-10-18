//! Language decision: the engine chain (learned corrections → mixed-word splitting → heuristic scores)
//! and the Japanese segmenter and converter interfaces the chain and the session use.

pub mod codeswitch;
#[cfg(feature = "japanese")]
pub mod convert;
pub mod converter;
pub mod engine;
#[cfg(feature = "japanese")]
pub mod kanji;
pub mod learning;
pub mod mixed;
#[cfg(feature = "japanese")]
pub mod segmenter;
pub mod shape;
