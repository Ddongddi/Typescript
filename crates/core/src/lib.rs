//! Typescript core: decides per word whether QWERTY keystrokes were meant as English or
//! Korean (2-set), and drives the typing session on top of that. No OS dependencies.

pub mod converter;
pub mod engine;
pub mod english;
pub mod hangul;
pub mod history;
pub mod korean;
pub mod learning;
pub mod mixed;
pub mod policy;
pub mod session;
mod sorted;
mod syllables;
pub mod testing;

use std::path::PathBuf;
use std::sync::Arc;

pub use converter::TextConverter;
pub use engine::{Decision, DecisionContext, DecisionEngine, HeuristicEngine, Language};
pub use english::EnglishModel;
pub use history::History;
pub use learning::LearningEngine;
pub use mixed::{MixedWord, SplittingEngine};
pub use policy::{AppPolicies, AppPolicy, Settings};
pub use session::{InputSession, Key, Range, TextClient};

/// `%APPDATA%\Typescript` (settings.json, apps.json, lexicon.json, user_history.json).
pub fn support_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Typescript"))
}

/// The engine chain of the original `Runtime.makeEngine`:
/// learned corrections → mixed-word splitting → heuristic scores.
pub fn make_engine(store_path: Option<PathBuf>) -> Arc<LearningEngine> {
    let english = EnglishModel::shared();
    let splitting = SplittingEngine::new(Box::new(HeuristicEngine::new()), english);
    Arc::new(LearningEngine::new(Box::new(splitting), store_path))
}
