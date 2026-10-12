//! Words the user has typed and how often (user_history.json), the first source of Tab
//! completions; the built-in word lists are the fallback (`suggest`).
//!
//! Several processes share the file, so a flush re-reads it and adds only this process's new
//! counts. ponytail: no cross-process lock, the last write wins; a named mutex if that matters.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::engine::Language;
use crate::english::EnglishModel;
use crate::korean::{continues, KoreanModel};
use crate::support_dir;

/// A word has to be typed this often before it is suggested: a one-off (a password typed
/// into a field that isn't masked) never comes back.
const MIN_COUNT: u32 = 2;
/// Written to disk after this many new words, and when the text field changes.
const FLUSH_EVERY: u32 = 10;
const MAX_WORD_CHARS: usize = 32;

#[derive(Default, Serialize, Deserialize)]
struct Store {
    words: BTreeMap<String, u32>,
}

#[derive(Default)]
struct State {
    store: Store,
    /// Counts added since the last flush.
    fresh: BTreeMap<String, u32>,
}

pub struct History {
    path: Option<PathBuf>,
    state: Mutex<State>,
}

fn read(path: Option<&PathBuf>) -> Store {
    path.and_then(|p| fs::read(p).ok()).and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}

/// Hangul syllables or ASCII letters only, 2..=32 characters: no numbers, no mixed tokens.
fn is_recordable(word: &str) -> bool {
    let n = word.chars().count();
    (2..=MAX_WORD_CHARS).contains(&n)
        && (word.chars().all(|c| ('가'..='힣').contains(&c)) || word.chars().all(|c| c.is_ascii_alphabetic()))
}

impl History {
    pub fn default_store_path() -> Option<PathBuf> {
        support_dir().map(|d| d.join("user_history.json"))
    }

    pub fn new(path: Option<PathBuf>) -> Self {
        let store = read(path.as_ref());
        Self { path, state: Mutex::new(State { store, fresh: BTreeMap::new() }) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Counts a committed word (anything that isn't a plain Korean or English word is ignored).
    pub fn record(&self, word: &str) {
        if !is_recordable(word) {
            return;
        }
        let pending = {
            let mut s = self.lock();
            *s.store.words.entry(word.to_string()).or_default() += 1;
            *s.fresh.entry(word.to_string()).or_default() += 1;
            s.fresh.values().sum::<u32>()
        };
        if pending >= FLUSH_EVERY {
            self.flush();
        }
    }

    /// Writes the new counts on top of whatever the file holds now. Failing to save (read-only
    /// profile, sandboxed app) only costs the learning.
    pub fn flush(&self) {
        let mut s = self.lock();
        let Some(path) = &self.path else { return };
        if s.fresh.is_empty() {
            return;
        }
        let mut merged = read(Some(path));
        for (word, n) in &s.fresh {
            *merged.words.entry(word.clone()).or_default() += n;
        }
        let Ok(json) = serde_json::to_vec_pretty(&merged) else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".{}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        if fs::write(&tmp, json).is_ok() && fs::rename(&tmp, path).is_ok() {
            s.store = merged;
            s.fresh.clear();
        } else {
            let _ = fs::remove_file(&tmp);
        }
    }

    /// Picks up what other processes wrote since this one loaded the file (every application
    /// runs its own copy of the text service). Words counted here but not yet written stay.
    pub fn reload(&self) {
        let mut s = self.lock();
        let mut store = read(self.path.as_ref());
        for (word, n) in &s.fresh {
            *store.words.entry(word.clone()).or_default() += n;
        }
        s.store = store;
    }

    /// The word typed most often that continues `prefix` (typed at least `MIN_COUNT` times,
    /// and more often than `prefix` itself).
    pub fn suggest(&self, prefix: &str) -> Option<String> {
        let s = self.lock();
        let own = s.store.words.get(prefix).copied().unwrap_or(0);
        let head = &prefix[..prefix.len() - prefix.chars().next_back()?.len_utf8()];
        let (word, count) = s
            .store
            .words
            .range(head.to_string()..)
            .take_while(|(w, _)| w.starts_with(head))
            .filter(|(w, _)| w.as_str() != prefix && continues(prefix, w))
            .max_by_key(|(w, n)| (**n, std::cmp::Reverse(w.len())))?;
        (*count >= MIN_COUNT && *count > own).then(|| word.clone())
    }
}

/// What Tab completes `prefix` (as shown in `language`) to: the user's own words first, then
/// the built-in lists. Mixed words (github에) have no completion.
pub fn suggest(history: Option<&History>, language: Language, prefix: &str) -> Option<String> {
    if prefix.is_empty() || language == Language::Mixed {
        return None;
    }
    history.and_then(|h| h.suggest(prefix)).or_else(|| match language {
        Language::English => EnglishModel::shared().complete(prefix),
        _ => KoreanModel::default().complete(prefix),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("typescript-history-{name}-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        path
    }

    #[test]
    fn suggests_the_most_typed_word_after_it_was_typed_twice() {
        let h = History::new(None);
        h.record("zebrafish");
        assert_eq!(h.suggest("zebra"), None);
        h.record("zebrafish");
        h.record("zebrastripe");
        assert_eq!(h.suggest("zebra").as_deref(), Some("zebrafish"));
    }

    #[test]
    fn completes_a_syllable_that_is_still_being_typed() {
        let h = History::new(None);
        for _ in 0..2 {
            h.record("안녕하세요");
        }
        for half in ["안녀", "안녕하", "안녕ㅎ", "안녕"] {
            assert_eq!(h.suggest(half).as_deref(), Some("안녕하세요"), "{half}");
        }
        assert_eq!(h.suggest("안누"), None);
    }

    #[test]
    fn a_prefix_typed_more_often_than_its_continuation_is_left_alone() {
        let h = History::new(None);
        for _ in 0..5 {
            h.record("zebra");
        }
        for _ in 0..2 {
            h.record("zebrafish");
        }
        assert_eq!(h.suggest("zebra"), None);
    }

    #[test]
    fn reload_shows_words_another_process_learned() {
        let path = temp("reload");
        let notepad = History::new(Some(path.clone()));
        let browser = History::new(Some(path.clone())); // already running when the words were typed
        notepad.record("zebrafish");
        notepad.record("zebrafish");
        notepad.flush();
        assert_eq!(browser.suggest("zebra"), None);
        browser.record("zebrastripe"); // counted here, not yet written
        browser.reload();
        let _ = fs::remove_file(&path);
        assert_eq!(browser.suggest("zebra").as_deref(), Some("zebrafish"));
        assert_eq!(browser.lock().store.words.get("zebrastripe"), Some(&1));
    }

    #[test]
    fn ignores_numbers_mixed_tokens_and_single_letters() {
        let h = History::new(None);
        for w in ["abc123", "x", "hello월드", "a-b", &"z".repeat(40)] {
            h.record(w);
            h.record(w);
        }
        assert!(h.lock().store.words.is_empty());
    }

    #[test]
    fn flush_merges_with_what_another_process_wrote() {
        let path = temp("merge");
        let a = History::new(Some(path.clone()));
        let b = History::new(Some(path.clone()));
        a.record("zebrafish");
        a.flush();
        b.record("zebrafish");
        b.flush();
        let c = History::new(Some(path.clone()));
        let _ = fs::remove_file(&path);
        assert_eq!(c.lock().store.words.get("zebrafish"), Some(&2));
    }

    #[test]
    fn built_in_lists_complete_when_the_user_has_no_history() {
        let english = suggest(None, Language::English, "progra").unwrap();
        assert!(english.starts_with("progra") && english.len() > 6, "{english}");
        let korean = suggest(None, Language::Korean, "안녕").unwrap();
        assert!(korean.starts_with("안녕") && korean.chars().count() > 2, "{korean}");
        assert_eq!(suggest(None, Language::English, "the"), None);
        assert_eq!(suggest(None, Language::Mixed, "github에"), None);
    }
}
