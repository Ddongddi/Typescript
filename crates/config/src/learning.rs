//! What the input method has learned about the user, shown in the dashboard and forgotten from
//! it: the words taught by correcting them twice (lexicon.json) and the words counted for Tab
//! completion (user_history.json). Both sit next to settings.json.

use std::path::PathBuf;

use serde_json::{json, Value};
use typescript_core::decide::learning::Store;
use typescript_core::store::{load_or_quarantine, persist};
use typescript_core::History;

use crate::env::Env;
use crate::error::{err, Result};

/// A word is suggested only after being typed this often (the rule in `History`).
const SUGGEST_AFTER: u32 = 2;
/// Corrected readings kept per reading by the kanji conversion.
const KANJI_PER_READING: u32 = 5;
/// The list stops here; the rest is counted.
const SHOWN: usize = 200;

fn lexicon_path(env: &Env) -> PathBuf {
    env.settings.with_file_name("lexicon.json")
}

fn history_path(env: &Env) -> PathBuf {
    env.settings.with_file_name("user_history.json")
}

/// The learned words (up to `SHOWN`), how many are waiting for a second correction, and how many
/// words Tab completion remembers.
pub fn summary(env: &Env) -> Value {
    let store: Store = load_or_quarantine(Some(&lexicon_path(env)));
    let history = History::new(Some(history_path(env)));
    let learned: Vec<Value> = store
        .learned
        .iter()
        .take(SHOWN)
        .map(|(keys, language)| json!({ "keys": keys, "language": language.id() }))
        .collect();
    json!({
        "learned": learned,
        "learnedCount": store.learned.len(),
        "pendingCount": store.once.len(),
        "historyWords": history.word_count(),
        "historySuggestible": history.suggestible_count(),
        "suggestAfter": SUGGEST_AFTER,
        "kanjiPerReading": KANJI_PER_READING,
    })
}

/// `{"action": "forget", "keys": "…"}`, `"forgetAll"` (taught words and the ones waiting), or
/// `"clearHistory"`.
pub fn change(env: &Env, body: &Value) -> Result<()> {
    match body["action"].as_str() {
        Some("forget") => {
            let keys = body["keys"].as_str().unwrap_or("");
            let path = lexicon_path(env);
            let mut store: Store = load_or_quarantine(Some(&path));
            let removed = store.learned.remove(keys).is_some() | store.once.remove(keys).is_some();
            if !removed {
                return err(format!("'{keys}' is not a learned word"));
            }
            save(&path, &store)
        }
        Some("forgetAll") => save(&lexicon_path(env), &Store::default()),
        Some("clearHistory") => {
            History::new(Some(history_path(env))).clear();
            Ok(())
        }
        _ => err("action has to be forget, forgetAll or clearHistory"),
    }
}

fn save(path: &std::path::Path, store: &Store) -> Result<()> {
    persist(path, store).map_err(|e| crate::error::Error(format!("could not save: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fixture;
    use typescript_core::Language;

    #[test]
    fn taught_words_are_listed_and_forgotten_and_the_history_is_cleared() {
        let f = fixture("learning");
        let mut store = Store::default();
        store.learned.insert("rjtm".into(), Language::Korean);
        store.once.insert("hello".into(), Language::Korean);
        persist(&lexicon_path(&f.env), &store).unwrap();
        let history = History::new(Some(history_path(&f.env)));
        for _ in 0..2 {
            history.record("zebrafish");
        }
        history.record("zebrastripe");
        history.flush();

        let s = summary(&f.env);
        assert_eq!(s["learnedCount"], 1);
        assert_eq!(s["pendingCount"], 1);
        assert_eq!(s["learned"][0]["keys"], "rjtm");
        assert_eq!(s["learned"][0]["language"], "ko");
        assert_eq!(s["historyWords"], 2);
        assert_eq!(s["historySuggestible"], 1);

        change(&f.env, &json!({ "action": "forget", "keys": "rjtm" })).unwrap();
        assert!(change(&f.env, &json!({ "action": "forget", "keys": "rjtm" })).is_err());
        assert_eq!(summary(&f.env)["learnedCount"], 0);
        change(&f.env, &json!({ "action": "forgetAll" })).unwrap();
        assert_eq!(summary(&f.env)["pendingCount"], 0);
        change(&f.env, &json!({ "action": "clearHistory" })).unwrap();
        assert_eq!(summary(&f.env)["historyWords"], 0);
        assert!(change(&f.env, &json!({ "action": "x" })).is_err());
    }
}
