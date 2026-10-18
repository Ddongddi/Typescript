//! What the input method has learned about the user, shown in the dashboard and forgotten from
//! it: the words taught by correcting them twice (lexicon.json) and the words counted for Tab
//! completion (user_history.json). Both sit next to settings.json.

use std::path::PathBuf;

use serde_json::{json, Value};
use tacit_core::decide::learning::Store;
use tacit_core::store::{load_or_quarantine, persist};
use tacit_core::{History, Language};

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
pub fn summary(env: &Env, render: impl Fn(&str, Language) -> Option<String>) -> Value {
    let store: Store = load_or_quarantine(Some(&lexicon_path(env)));
    let history = History::new(Some(history_path(env)));
    // a–z whatever the case; `out` is what the word turns into (none when its pack is not here)
    let mut words: Vec<_> = store.learned.iter().collect();
    words.sort_by_cached_key(|(keys, _)| (keys.to_lowercase(), keys.to_string()));
    let learned: Vec<Value> = words
        .into_iter()
        .take(SHOWN)
        .map(|(keys, language)| {
            json!({ "keys": keys, "language": language.id(), "out": render(keys, *language) })
        })
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
    fn plain(keys: &str, _: Language) -> Option<String> {
        Some(format!("<{keys}>"))
    }

    #[test]
    fn taught_words_are_listed_and_forgotten_and_the_history_is_cleared() {
        let f = fixture("learning");
        let mut store = Store::default();
        store.learned.insert("rjtm".into(), Language::Korean);
        store.learned.insert("Zed".into(), Language::Korean);
        store.learned.insert("apple".into(), Language::Korean);
        store.once.insert("hello".into(), Language::Korean);
        persist(&lexicon_path(&f.env), &store).unwrap();
        let history = History::new(Some(history_path(&f.env)));
        for _ in 0..2 {
            history.record("zebrafish");
        }
        history.record("zebrastripe");
        history.flush();

        let s = summary(&f.env, plain);
        assert_eq!(s["learnedCount"], 3);
        assert_eq!(s["pendingCount"], 1);
        let order: Vec<_> = s["learned"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["keys"].clone())
            .collect();
        assert_eq!(order, ["apple", "rjtm", "Zed"]);
        assert_eq!(s["learned"][1]["language"], "ko");
        assert_eq!(s["learned"][1]["out"], "<rjtm>");
        assert_eq!(s["historyWords"], 2);
        assert_eq!(s["historySuggestible"], 1);

        change(&f.env, &json!({ "action": "forget", "keys": "rjtm" })).unwrap();
        assert!(change(&f.env, &json!({ "action": "forget", "keys": "rjtm" })).is_err());
        assert_eq!(summary(&f.env, plain)["learnedCount"], 2);
        change(&f.env, &json!({ "action": "forgetAll" })).unwrap();
        assert_eq!(summary(&f.env, plain)["pendingCount"], 0);
        change(&f.env, &json!({ "action": "clearHistory" })).unwrap();
        assert_eq!(summary(&f.env, plain)["historyWords"], 0);
        assert!(change(&f.env, &json!({ "action": "x" })).is_err());
    }
}
