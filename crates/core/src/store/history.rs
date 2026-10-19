//! Words the user has typed and how often (user_history.json), the first source of Tab
//! completions; the built-in word lists are the fallback (`suggest`).
//!
//! Several processes share the file, so a flush re-reads it and adds only this process's new
//! counts. ponytail: no cross-process lock, the last write wins; a named mutex if that matters.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::decide::engine::Language;
use crate::lang::english::EnglishModel;
use crate::lang::korean::KoreanModel;
use crate::support_dir;

/// A word has to be typed this often before it is suggested: a one-off (a password typed
/// into a field that isn't masked) never comes back.
const MIN_COUNT: u32 = 2;
/// Written to disk after this many new words, and when the text field changes.
const FLUSH_EVERY: u32 = 10;
const MAX_WORD_CHARS: usize = 32;
/// The file never holds more words than this: past it the least typed ones go, so a flood of
/// distinct words cannot grow it (and every rewrite of it) without bound.
const MAX_WORDS: usize = 20_000;

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
    crate::store::load_or_quarantine(path.map(PathBuf::as_path))
}

/// Hangul syllables or ASCII letters only, 2..=32 characters: no numbers, no mixed tokens.
fn is_recordable(word: &str) -> bool {
    let n = word.chars().count();
    (2..=MAX_WORD_CHARS).contains(&n)
        && (word.chars().all(|c| ('가'..='힣').contains(&c))
            || word.chars().all(|c| c.is_ascii_alphabetic()))
}

/// Keeps the `MAX_WORDS` most typed words (ties: the alphabetically first).
fn prune(store: &mut Store) {
    if store.words.len() <= MAX_WORDS {
        return;
    }
    // Selecting the cut is linear; only the kept words are then collected.
    let mut ranked: Vec<(&String, &u32)> = store.words.iter().collect();
    ranked.select_nth_unstable_by(MAX_WORDS, |a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let keep: std::collections::BTreeSet<String> = ranked[..MAX_WORDS]
        .iter()
        .map(|(w, _)| (*w).clone())
        .collect();
    store.words.retain(|w, _| keep.contains(w));
}

impl History {
    pub fn default_store_path() -> Option<PathBuf> {
        support_dir().map(|d| d.join("user_history.json"))
    }

    pub fn new(path: Option<PathBuf>) -> Self {
        let store = read(path.as_ref());
        Self {
            path,
            state: Mutex::new(State {
                store,
                fresh: BTreeMap::new(),
            }),
        }
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
        prune(&mut merged);
        if crate::store::persist(path, &merged).is_ok() {
            s.store = merged;
            s.fresh.clear();
        }
    }

    /// How many different words are remembered (typed once or more).
    pub fn word_count(&self) -> usize {
        self.lock().store.words.len()
    }

    /// How many of them have been typed often enough to be suggested.
    pub fn suggestible_count(&self) -> usize {
        self.lock()
            .store
            .words
            .values()
            .filter(|n| **n >= MIN_COUNT)
            .count()
    }

    /// Forgets every word, here and in the file.
    pub fn clear(&self) {
        let mut s = self.lock();
        s.fresh.clear();
        s.store = Store::default();
        if let Some(path) = &self.path {
            let _ = crate::store::persist(path, &s.store);
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
        if s.store.words.is_empty() {
            return None;
        }
        let own = s.store.words.get(prefix).copied().unwrap_or(0);
        let last = prefix.chars().next_back()?;
        let head = &prefix[..prefix.len() - last.len_utf8()];
        // Exact letters first, then the syllables an unfinished Hangul character can become.
        let syllables = crate::lang::hangul::cho_index(last)
            .map(|initial| ('가' as u32 + initial as u32 * 588, 588))
            .or_else(|| {
                (('가'..='힣').contains(&last) && (last as u32 - '가' as u32).is_multiple_of(28))
                    .then_some((last as u32 + 1, 27))
            })
            .map(|(start, count)| {
                let start_char = char::from_u32(start).unwrap();
                let end_char = char::from_u32(start + count).unwrap();
                (format!("{head}{start_char}"), format!("{head}{end_char}"))
            });
        let exact = s
            .store
            .words
            .range(prefix.to_string()..)
            .take_while(|(w, _)| w.starts_with(prefix));
        let (word, count) = exact
            .chain(
                syllables
                    .into_iter()
                    .flat_map(|(lo, hi)| s.store.words.range(lo..hi)),
            )
            .filter(|(w, _)| w.as_str() != prefix)
            .max_by_key(|(w, n)| (**n, std::cmp::Reverse(w.len())))?;
        (*count >= MIN_COUNT && *count > own).then(|| word.clone())
    }
}

/// What Tab completes `prefix` (as shown in `language`) to: the user's own words first, then
/// the built-in lists. Mixed words (github에) have no completion.
pub fn suggest(history: Option<&History>, language: Language, prefix: &str) -> Option<String> {
    if prefix.is_empty() || matches!(language, Language::Mixed | Language::Japanese) {
        return None;
    }
    history
        .and_then(|h| h.suggest(prefix))
        .or_else(|| match language {
            Language::English => EnglishModel::shared().complete(prefix),
            Language::Japanese => None,
            _ => KoreanModel::default().complete(prefix),
        })
}

#[cfg(test)]
mod tests {
    use crate::lang::korean::continues;

    #[test]
    fn bounded_completion_matches_the_full_scan_including_ties_and_partial_hangul() {
        let history = History::new(None);
        {
            let mut state = history.lock();
            for c in '가'..='힣' {
                state.store.words.insert(format!("{c}가"), 2 + c as u32 % 7);
            }
            for word in [
                "program",
                "project",
                "pro",
                "ㄱ가",
                "안녕하세요",
                "안녕하셔요",
                "힣힣",
            ] {
                state.store.words.insert(word.to_string(), 4);
            }
        }
        let prefixes = ["p", "pro", "", "각", "힣", "안녀", "안녕ㅎ", "z"]
            .into_iter()
            .map(String::from)
            .chain(
                "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ"
                    .chars()
                    .map(|c| c.to_string()),
            )
            .chain(('가'..='힣').step_by(28).map(|c| c.to_string()));
        for prefix in prefixes {
            let expected = {
                let state = history.lock();
                let own = state.store.words.get(&prefix).copied().unwrap_or(0);
                state
                    .store
                    .words
                    .iter()
                    .filter(|(word, _)| word.as_str() != prefix && continues(&prefix, word))
                    .max_by_key(|(word, count)| (**count, std::cmp::Reverse(word.len())))
                    .filter(|(_, count)| **count >= MIN_COUNT && **count > own)
                    .map(|(word, _)| word.clone())
            };
            // Empty input has always meant no suggestion in History::suggest.
            assert_eq!(
                history.suggest(&prefix),
                if prefix.is_empty() { None } else { expected },
                "{prefix}"
            );
        }
    }

    use std::fs;

    #[test]
    fn the_file_is_capped_and_keeps_the_most_typed_words() {
        let mut store = Store::default();
        for i in 0..MAX_WORDS + 50 {
            store.words.insert(format!("w{i:06}"), 2);
        }
        store.words.insert("favourite".into(), 9);
        prune(&mut store);
        assert_eq!(store.words.len(), MAX_WORDS);
        assert!(store.words.contains_key("favourite"));
    }

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tacit-history-{name}-{}.json", std::process::id()));
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
    fn a_word_of_32_characters_is_kept_and_one_of_33_is_not() {
        let h = History::new(None);
        for w in ["a".repeat(MAX_WORD_CHARS), "a".repeat(MAX_WORD_CHARS + 1)] {
            h.record(&w);
            h.record(&w);
        }
        assert!(h
            .lock()
            .store
            .words
            .contains_key(&"a".repeat(MAX_WORD_CHARS)));
        assert!(!h
            .lock()
            .store
            .words
            .contains_key(&"a".repeat(MAX_WORD_CHARS + 1)));
    }

    #[test]
    fn a_damaged_history_file_is_set_aside_instead_of_overwritten() {
        let path = temp("damaged");
        std::fs::write(&path, b"{ broken").unwrap();
        let h = History::new(Some(path.clone()));
        h.record("zebrafish");
        h.record("zebrafish");
        h.flush();
        let mut aside = path.as_os_str().to_owned();
        aside.push(".unreadable");
        assert_eq!(std::fs::read(&aside).unwrap(), b"{ broken");
        assert_eq!(h.suggest("zebra").as_deref(), Some("zebrafish"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&aside);
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
        assert!(
            english.starts_with("progra") && english.len() > 6,
            "{english}"
        );
        let korean = suggest(None, Language::Korean, "안녕").unwrap();
        assert!(
            korean.starts_with("안녕") && korean.chars().count() > 2,
            "{korean}"
        );
        assert_eq!(suggest(None, Language::English, "the"), None);
        assert_eq!(suggest(None, Language::Mixed, "github에"), None);
    }
}
