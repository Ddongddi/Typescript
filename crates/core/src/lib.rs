//! tacit core: decides per word whether QWERTY keystrokes were meant as English,
//! Korean (2-set) or Japanese (romaji), and drives the typing session on top of that.
//! No OS dependencies.

pub mod decide;
pub mod lang;
pub mod session;
pub mod store;

use std::path::PathBuf;
use std::sync::Arc;

#[cfg(feature = "japanese")]
pub use decide::convert::{ConversionCandidate, JapaneseConverter, KanaConverter};
pub use decide::converter::TextConverter;
pub use decide::engine::{Decision, DecisionContext, DecisionEngine, HeuristicEngine, Language};
#[cfg(feature = "japanese")]
pub use decide::kanji::{DictionaryConverter, KanjiDictionary};
pub use decide::learning::LearningEngine;
pub use decide::mixed::{MixedWord, SplittingEngine};
#[cfg(feature = "japanese")]
pub use decide::segmenter::{JapaneseSegmenter, Segment};
pub use lang::english::EnglishModel;
#[cfg(feature = "japanese")]
pub use lang::japanese::JapaneseModel;
pub use lang::pack;
#[cfg(feature = "japanese")]
pub use lang::romaji::RomajiComposer;
pub use lang::set::Languages;
#[cfg(feature = "japanese")]
pub use session::Conversion;
pub use session::{CandidatePage, InputSession, Key, Range, TextClient};
pub use store::history::History;
pub use store::policy::{AppPolicies, AppPolicy, Settings};

/// The systems the per-user folders differ on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Linux
        }
    }
}

type Var<'a> = &'a dyn Fn(&str) -> Option<std::ffi::OsString>;

fn var(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name).filter(|v| !v.is_empty())
}

fn mac_support(get: Var) -> Option<PathBuf> {
    let home = PathBuf::from(get("HOME")?);
    Some(
        home.join("Library")
            .join("Application Support")
            .join("tacit"),
    )
}

/// Settings and learned data: `%APPDATA%\tacit` on Windows, `~/Library/Application
/// Support/tacit` on macOS, `$XDG_CONFIG_HOME/tacit` (else `~/.config/tacit`)
/// elsewhere. Holds settings.json, apps.json, lexicon.json, user_history.json.
pub fn support_dir() -> Option<PathBuf> {
    support_dir_for(Os::current(), &var)
}

fn support_dir_for(os: Os, get: Var) -> Option<PathBuf> {
    match os {
        Os::Windows => get("APPDATA").map(|d| PathBuf::from(d).join("tacit")),
        Os::Mac => mac_support(get),
        Os::Linux => match get("XDG_CONFIG_HOME") {
            Some(d) => Some(PathBuf::from(d).join("tacit")),
            None => Some(PathBuf::from(get("HOME")?).join(".config").join("tacit")),
        },
    }
}

/// Where installed language packs live. Windows: `%LOCALAPPDATA%\tacit\packs`, and the
/// same path under the user profile. In an AppContainer process (a Store app, a sandboxed
/// browser) the first is usually redirected to the package's own folder, which has no packs;
/// the second is where the installer put them. Both are scanned. The folder is in the user's
/// local profile, not the roaming one, so the installer can grant AppContainer read access to
/// it. macOS: next to the settings. Elsewhere: `$XDG_DATA_HOME/tacit/packs` (else
/// `~/.local/share/tacit/packs`).
pub fn pack_dirs() -> Vec<PathBuf> {
    pack_dirs_for(Os::current(), &var)
}

fn pack_dirs_for(os: Os, get: Var) -> Vec<PathBuf> {
    match os {
        Os::Windows => pack_dirs_from(get("LOCALAPPDATA"), get("USERPROFILE")),
        Os::Mac => mac_support(get)
            .map(|d| d.join("packs"))
            .into_iter()
            .collect(),
        Os::Linux => {
            let base = match get("XDG_DATA_HOME") {
                Some(d) => Some(PathBuf::from(d)),
                None => get("HOME").map(|h| PathBuf::from(h).join(".local").join("share")),
            };
            base.map(|b| b.join("tacit").join("packs"))
                .into_iter()
                .collect()
        }
    }
}

fn pack_dirs_from(
    local: Option<std::ffi::OsString>,
    profile: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    let under = |base: PathBuf| base.join("tacit").join("packs");
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
    #[cfg(feature = "japanese")]
    let segmenter = heuristic.segmenter.clone();
    let engine: Box<dyn DecisionEngine> = if languages.contains(Language::English) {
        let mut splitting = SplittingEngine::new(Box::new(heuristic), EnglishModel::shared());
        splitting.korean_on = languages.contains(Language::Korean);
        #[cfg(feature = "japanese")]
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

    fn local_app_data(profile: &str) -> PathBuf {
        PathBuf::from(profile).join("AppData").join("Local")
    }

    #[test]
    fn pack_dirs_cover_the_redirected_local_app_data() {
        let redirected = PathBuf::from("u").join("Packages").join("pfn").join("AC");
        let dirs = pack_dirs_from(Some(redirected.into()), Some("u".into()));
        assert_eq!(dirs.len(), 2);
        assert!(dirs[1].starts_with(local_app_data("u")));
        assert!(dirs[1].ends_with("tacit/packs"));
    }

    #[test]
    fn pack_dirs_do_not_repeat_the_usual_location() {
        let dirs = pack_dirs_from(Some(local_app_data("u").into()), Some("u".into()));
        assert_eq!(dirs.len(), 1);
        assert!(pack_dirs_from(None, None).is_empty());
    }

    fn vars(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<std::ffi::OsString> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).into())
        }
    }

    #[test]
    fn folders_follow_each_systems_convention() {
        let linux = vars(&[("HOME", "/home/a")]);
        assert_eq!(
            support_dir_for(Os::Linux, &linux),
            Some(PathBuf::from("/home/a/.config/tacit"))
        );
        assert_eq!(
            pack_dirs_for(Os::Linux, &linux),
            vec![PathBuf::from("/home/a/.local/share/tacit/packs")]
        );
        let xdg = vars(&[
            ("HOME", "/h"),
            ("XDG_CONFIG_HOME", "/c"),
            ("XDG_DATA_HOME", "/d"),
        ]);
        assert_eq!(
            support_dir_for(Os::Linux, &xdg),
            Some(PathBuf::from("/c/tacit"))
        );
        assert_eq!(
            pack_dirs_for(Os::Linux, &xdg),
            vec![PathBuf::from("/d/tacit/packs")]
        );
        let mac = vars(&[("HOME", "/Users/a")]);
        assert_eq!(
            support_dir_for(Os::Mac, &mac),
            Some(PathBuf::from("/Users/a/Library/Application Support/tacit"))
        );
        assert_eq!(
            pack_dirs_for(Os::Mac, &mac),
            vec![PathBuf::from(
                "/Users/a/Library/Application Support/tacit/packs"
            )]
        );
        let none = vars(&[]);
        assert_eq!(support_dir_for(Os::Linux, &none), None);
        assert!(pack_dirs_for(Os::Mac, &none).is_empty());
        let win = vars(&[("APPDATA", "C:/A")]);
        assert_eq!(
            support_dir_for(Os::Windows, &win),
            Some(PathBuf::from("C:/A/tacit"))
        );
    }
}
