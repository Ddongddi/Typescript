//! Process-wide models: every text service in a process shares its engines, converter and word
//! history. The Japanese chain is built only once some session asks for it.

use std::sync::{Arc, OnceLock};

use typescript_core::{make_engine, History, JapaneseConverter, KanaConverter, LearningEngine};

static ENGINE: OnceLock<Arc<LearningEngine>> = OnceLock::new();
static ENGINE_JA: OnceLock<Arc<LearningEngine>> = OnceLock::new();

/// One engine per process: models and learned words are shared by every text service in it.
/// The Japanese model is heavy (and lazy), so a process that never turns Japanese on never
/// pays for it.
pub fn engine(japanese: bool) -> Arc<LearningEngine> {
    if japanese {
        ENGINE_JA
            .get_or_init(|| make_engine(LearningEngine::default_store_path(), true))
            .clone()
    } else {
        ENGINE
            .get_or_init(|| make_engine(LearningEngine::default_store_path(), false))
            .clone()
    }
}

/// Re-reads the lexicon for the engines that exist in this process (settings/lexicon edits
/// made in another process or by hand).
pub fn reload_engines() {
    if let Some(engine) = ENGINE.get() {
        engine.reload();
    }
    if let Some(engine) = ENGINE_JA.get() {
        engine.reload();
    }
}

static CONVERTER: OnceLock<Arc<dyn JapaneseConverter>> = OnceLock::new();

/// The kana→kanji engine for this process. The macOS engines (libmecabra, AzooKey) have no
/// Windows counterpart yet, so this is `KanaConverter`: the reading in hiragana and katakana.
/// See `typescript_core::decide::convert`.
pub fn converter() -> Arc<dyn JapaneseConverter> {
    CONVERTER
        .get_or_init(|| Arc::new(KanaConverter::new()))
        .clone()
}

static HISTORY: OnceLock<Arc<History>> = OnceLock::new();

/// One word history per process, like the engine: every text service in it counts into it.
pub fn history() -> Arc<History> {
    HISTORY
        .get_or_init(|| Arc::new(History::new(History::default_store_path())))
        .clone()
}
