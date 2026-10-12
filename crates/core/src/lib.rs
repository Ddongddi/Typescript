//! Typescript core: decides per word whether QWERTY keystrokes were meant as English or
//! Korean (2-set), and drives the typing session on top of that. No OS dependencies.

pub mod decide;
pub mod lang;
pub mod session;
pub mod store;

use std::path::PathBuf;
use std::sync::Arc;

pub use decide::converter::TextConverter;
pub use decide::engine::{Decision, DecisionContext, DecisionEngine, HeuristicEngine, Language};
pub use decide::learning::LearningEngine;
pub use decide::mixed::{MixedWord, SplittingEngine};
pub use lang::english::EnglishModel;
pub use session::{InputSession, Key, Range, TextClient};
pub use store::history::History;
pub use store::policy::{AppPolicies, AppPolicy, Settings};

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
