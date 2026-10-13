//! Typescript core: decides per word whether QWERTY keystrokes were meant as English,
//! Korean (2-set) or Japanese (romaji), and drives the typing session on top of that.
//! No OS dependencies.

pub mod decide;
pub mod lang;
pub mod session;
pub mod store;

use std::path::PathBuf;
use std::sync::Arc;

pub use decide::convert::{ConversionCandidate, JapaneseConverter, KanaConverter};
pub use decide::converter::TextConverter;
pub use decide::engine::{Decision, DecisionContext, DecisionEngine, HeuristicEngine, Language};
pub use decide::learning::LearningEngine;
pub use decide::mixed::{MixedWord, SplittingEngine};
pub use decide::segmenter::{JapaneseSegmenter, Segment};
pub use lang::english::EnglishModel;
pub use lang::japanese::JapaneseModel;
pub use lang::romaji::RomajiComposer;
pub use session::{Conversion, InputSession, Key, Range, TextClient};
pub use store::history::History;
pub use store::policy::{AppPolicies, AppPolicy, Settings};

/// `%APPDATA%\Typescript` (settings.json, apps.json, lexicon.json, user_history.json).
pub fn support_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Typescript"))
}

/// The engine chain of the original `Runtime.makeEngine`:
/// learned corrections → mixed-word splitting → heuristic scores.
/// `japanese`: score romaji Japanese too (the app's `Settings.japanese`).
pub fn make_engine(store_path: Option<PathBuf>, japanese: bool) -> Arc<LearningEngine> {
    let english = EnglishModel::shared();
    let heuristic = if japanese {
        HeuristicEngine::with_japanese(english, JapaneseModel::shared())
    } else {
        HeuristicEngine::new()
    };
    let segmenter = heuristic.segmenter.clone();
    let mut splitting = SplittingEngine::new(Box::new(heuristic), english);
    if let Some(segmenter) = segmenter {
        splitting = splitting.with_japanese(segmenter);
    }
    Arc::new(LearningEngine::new(Box::new(splitting), store_path))
}
