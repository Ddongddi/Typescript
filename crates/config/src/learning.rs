//! What the input method has learned about the user, shown in the dashboard and forgotten from
//! it: the words taught by correcting them twice (lexicon.json) and the words counted for Tab
//! completion (user_history.json). Both sit next to settings.json.

use std::path::PathBuf;

use serde_json::{json, Value};
use tacit_core::decide::learning::Store;
use tacit_core::store::{load_or_quarantine, lock_file, persist};
use tacit_core::{History, Language};

use crate::env::Env;
use crate::error::{err, Result};

/// A word is suggested only after being typed this often (the rule in `History`).
const SUGGEST_AFTER: u32 = 2;
/// Corrected readings kept per reading by the kanji conversion.
const KANJI_PER_READING: u32 = 5;
/// Maximum words rendered per page.
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
    page(env, render, "", 0)
}

/// Search the complete lexicon, returning one bounded page with global history counts.
pub fn page(
    env: &Env,
    render: impl Fn(&str, Language) -> Option<String>,
    query: &str,
    page: usize,
) -> Value {
    let store: Store = load_or_quarantine(Some(&lexicon_path(env)));
    let history = History::new(Some(history_path(env)));
    // a–z whatever the case; `out` is what the word turns into (none when its pack is not here)
    let mut words: Vec<_> = store.learned.iter().collect();
    words.sort_by_cached_key(|(keys, _)| (keys.to_lowercase(), keys.to_string()));
    let query = query.trim().to_lowercase();
    let words: Vec<_> = words
        .into_iter()
        .filter(|(keys, _)| keys.to_lowercase().contains(&query))
        .collect();
    let filtered_count = words.len();
    let pages = filtered_count.div_ceil(SHOWN).max(1);
    let page = page.min(pages - 1);
    let learned: Vec<Value> = words
        .into_iter()
        .skip(page * SHOWN)
        .take(SHOWN)
        .map(|(keys, language)| {
            json!({ "keys": keys, "language": language.id(), "out": render(keys, *language) })
        })
        .collect();
    json!({
        "learned": learned,
        "learnedCount": store.learned.len(),
        "filteredCount": filtered_count,
        "page": page,
        "pages": pages,
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
            let _file_lock = lock_file(&path)
                .map_err(|e| crate::error::Error(format!("could not lock learning: {e}")))?;
            let mut store: Store = load_or_quarantine(Some(&path));
            let removed = store.learned.remove(keys).is_some() | store.once.remove(keys).is_some();
            if !removed {
                return err(format!("'{keys}' is not a learned word"));
            }
            save(&path, &store)
        }
        Some("forgetAll") => {
            let path = lexicon_path(env);
            let _file_lock = lock_file(&path)
                .map_err(|e| crate::error::Error(format!("could not lock learning: {e}")))?;
            save(&path, &Store::default())
        }
        Some("clearHistory") => History::new(Some(history_path(env)))
            .clear()
            .map_err(|e| crate::error::Error(format!("could not clear history: {e}"))),
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
    fn search_and_pages_reach_words_after_the_first_two_hundred() {
        let f = fixture("learning-pages");
        let mut store = Store::default();
        for n in 0..205 {
            store
                .learned
                .insert(format!("word{n:03}"), Language::Korean);
        }
        persist(&lexicon_path(&f.env), &store).unwrap();
        let first = summary(&f.env, plain);
        assert_eq!(first["learned"].as_array().unwrap().len(), 200);
        assert_eq!(first["pages"], 2);
        let last = page(&f.env, plain, "", usize::MAX);
        assert_eq!(last["page"], 1);
        assert_eq!(last["learned"][0]["keys"], "word200");
        assert_eq!(last["learned"].as_array().unwrap().len(), 5);
        let found = page(&f.env, plain, " WORD204 ", 0);
        assert_eq!(found["filteredCount"], 1);
        assert_eq!(found["learned"][0]["keys"], "word204");
        change(&f.env, &json!({ "action": "forget", "keys": "word204" })).unwrap();
        let empty = page(&f.env, plain, "word204", 1);
        assert_eq!(empty["page"], 0);
        assert_eq!(empty["pages"], 1);
        assert_eq!(empty["filteredCount"], 0);
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
