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
pub use lang::pack;
pub use lang::romaji::RomajiComposer;
pub use lang::set::Languages;
pub use session::{Conversion, InputSession, Key, Range, TextClient};
pub use store::history::History;
pub use store::policy::{AppPolicies, AppPolicy, Settings};

/// `%APPDATA%\Typescript` (settings.json, apps.json, lexicon.json, user_history.json).
pub fn support_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Typescript"))
}

/// Where installed language packs live: `%LOCALAPPDATA%\Typescript\packs`, and the same path
/// under the user profile. In an AppContainer process (a Store app, a sandboxed browser) the
/// first is usually redirected to the package's own folder, which has no packs; the second is
/// where the installer put them. Both are scanned. The folder is in the user's local profile,
/// not the roaming one, so the installer can grant AppContainer read access to it.
pub fn pack_dirs() -> Vec<PathBuf> {
    pack_dirs_from(
        std::env::var_os("LOCALAPPDATA"),
        std::env::var_os("USERPROFILE"),
    )
}

fn pack_dirs_from(
    local: Option<std::ffi::OsString>,
    profile: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    let under = |base: PathBuf| base.join("Typescript").join("packs");
    let mut dirs: Vec<PathBuf> = local.into_iter().map(|d| under(d.into())).collect();
    if let Some(profile) = profile {
        let dir = under(PathBuf::from(profile).join("AppData").join("Local"));
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_dirs_cover_the_redirected_local_app_data() {
        let dirs = pack_dirs_from(
            Some(r"C:\Users\a\AppData\Local\Packages\pfn\AC".into()),
            Some(r"C:\Users\a".into()),
        );
        assert_eq!(dirs.len(), 2);
        assert!(dirs[1].ends_with(r"AppData\Local\Typescript\packs"));
    }

    #[test]
    fn pack_dirs_do_not_repeat_the_usual_location() {
        let dirs = pack_dirs_from(
            Some(r"C:\Users\a\AppData\Local".into()),
            Some(r"C:\Users\a".into()),
        );
        assert_eq!(dirs.len(), 1);
        assert!(pack_dirs_from(None, None).is_empty());
    }
}
