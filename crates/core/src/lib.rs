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
pub use decide::kanji::{DictionaryConverter, KanjiDictionary};
pub use decide::learning::LearningEngine;
pub use decide::mixed::{MixedWord, SplittingEngine};
pub use decide::segmenter::{JapaneseSegmenter, Segment};
pub use lang::english::EnglishModel;
pub use lang::japanese::JapaneseModel;
pub use lang::romaji::RomajiComposer;
pub use lang::set::Languages;
pub use session::{Conversion, InputSession, Key, Range, TextClient};
pub use store::history::History;
pub use store::policy::{AppPolicies, AppPolicy, Settings};

/// This build carries the Japanese models (the `japanese` feature). Without it Japanese is
/// never turned on, whatever the settings say.
pub const JAPANESE: bool = cfg!(feature = "japanese");

/// `%APPDATA%\Typescript` (settings.json, apps.json, lexicon.json, user_history.json).
pub fn support_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Typescript"))
}

/// The engine chain of the original `Runtime.makeEngine`:
/// learned corrections → mixed-word splitting → heuristic scores, over `languages`.
pub fn make_engine(store_path: Option<PathBuf>, languages: &Languages) -> Arc<LearningEngine> {
    let heuristic = HeuristicEngine::for_languages(languages);
    let segmenter = heuristic.segmenter.clone();
    let engine: Box<dyn DecisionEngine> = if languages.contains(Language::English) {
        let mut splitting = SplittingEngine::new(Box::new(heuristic), EnglishModel::shared());
        splitting.korean_on = languages.contains(Language::Korean);
        if let Some(segmenter) = segmenter {
            splitting = splitting.with_japanese(segmenter);
        }
        Box::new(splitting)
    } else {
        Box::new(heuristic)
    };
    Arc::new(LearningEngine::new(engine, store_path).with_languages(languages.clone()))
}
