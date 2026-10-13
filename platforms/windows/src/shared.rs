//! Process-wide models: every text service in a process shares its engines, converter and word
//! history. The Japanese chain is built only once some session asks for it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use typescript_core::{
    make_engine, DictionaryConverter, History, JapaneseConverter, Languages, LearningEngine,
};

static ENGINES: OnceLock<Mutex<HashMap<Languages, Arc<LearningEngine>>>> = OnceLock::new();

fn engines() -> MutexGuard<'static, HashMap<Languages, Arc<LearningEngine>>> {
    ENGINES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// One engine per language set per process: models and learned words are shared by every
/// text service in it. The Japanese model is heavy (and lazy), so a process that never turns
/// Japanese on never pays for it.
pub fn engine(languages: &Languages) -> Arc<LearningEngine> {
    engines()
        .entry(languages.clone())
        .or_insert_with(|| make_engine(LearningEngine::default_store_path(), languages))
        .clone()
}

/// Re-reads the lexicon for the engines that exist in this process (settings/lexicon edits
/// made in another process or by hand).
pub fn reload_engines() {
    for engine in engines().values() {
        engine.reload();
    }
}

/// Loads the installed packs this process doesn't have yet (on first use and whenever the
/// settings change: a language may just have been installed).
pub fn refresh_packs() {
    if let Some(root) = typescript_core::pack_dir() {
        typescript_core::pack::refresh(&root);
    }
}

static CONVERTER: OnceLock<Arc<dyn JapaneseConverter>> = OnceLock::new();

/// The kana→kanji engine for this process: the compact IPADIC dictionary (decide/kanji.rs),
/// with the user's choices learned into `%APPDATA%\Typescript\kanji_learning.json`.
pub fn converter() -> Arc<dyn JapaneseConverter> {
    CONVERTER
        .get_or_init(|| {
            Arc::new(DictionaryConverter::new(
                DictionaryConverter::default_learning_path(),
            ))
        })
        .clone()
}

static HISTORY: OnceLock<Arc<History>> = OnceLock::new();

/// One word history per process, like the engine: every text service in it counts into it.
pub fn history() -> Arc<History> {
    HISTORY
        .get_or_init(|| Arc::new(History::new(History::default_store_path())))
        .clone()
}
