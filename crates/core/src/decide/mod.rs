//! Language decision: the engine chain (learned corrections → mixed-word splitting → heuristic scores)
//! and the Japanese segmenter and converter interfaces the chain and the session use.

pub mod codeswitch;
pub mod convert;
pub mod converter;
pub mod engine;
pub mod kanji;
pub mod learning;
pub mod mixed;
pub mod segmenter;
pub mod shape;
