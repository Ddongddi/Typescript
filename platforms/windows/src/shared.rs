//! Process-wide models: every text service in a process shares one engine and one word history.

use std::sync::{Arc, OnceLock};

use typescript_core::{make_engine, History, LearningEngine};

static ENGINE: OnceLock<Arc<LearningEngine>> = OnceLock::new();

/// One engine per process: models and learned words are shared by every text service in it.
pub fn engine() -> Arc<LearningEngine> {
    ENGINE
        .get_or_init(|| make_engine(LearningEngine::default_store_path()))
        .clone()
}

static HISTORY: OnceLock<Arc<History>> = OnceLock::new();

/// One word history per process, like the engine: every text service in it counts into it.
pub fn history() -> Arc<History> {
    HISTORY
        .get_or_init(|| Arc::new(History::new(History::default_store_path())))
        .clone()
}
