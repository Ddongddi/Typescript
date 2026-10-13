//! Kana → kanji conversion over a compact IPADIC dictionary (the `JapaneseConverter` of
//! decide/convert.rs). Readings are looked up in a sorted hiragana index and rendered by the
//! best paths through the word lattice, scored with IPADIC's word costs and connection matrix
//! — the MeCab model (the connection formula was checked against MeCab's `connector.h`:
//! `matrix[left.rcAttr + lsize * right.lcAttr]`, BOS/EOS attributes 0).
//!
//! `tools/build_ipadic.py` builds `data/ipadic_compact.bin`; this module reads it zero-copy.
//! Seamtype gets kanji from macOS's engines (libmecabra, AzooKey); this is the Windows
//! counterpart: the best paths, their clause boundaries and the user's earlier choices become
//! the candidates the session cycles through.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use crate::decide::convert::{ConversionCandidate, JapaneseConverter};
use crate::lang::romaji::RomajiComposer;
use crate::support_dir;

const DICTIONARY: &[u8] = include_bytes!("../../data/ipadic_compact.bin");
const MAGIC: &[u8; 8] = b"TSIPDIC2";

/// No entry covers this character (a latin run, a stray mark): the path keeps it as typed.
const UNKNOWN_COST: i32 = 15_000;
/// Whole-reading paths offered, best first.
const FULL_CANDIDATES: usize = 8;
/// Clause boundaries of the best path, offered as first-clause candidates.
const PREFIX_CANDIDATES: usize = 4;
/// A reading no real word covers comes out as an odd multi-piece path (よーそろー → よー粗ロー at
/// 14124, きょうはよーそろー at 17168); readings that do have words, latin runs and phrases with
/// their unknown cost removed stay under 12000 in measurements (きょうはgoogleでけんさく 9019,
/// さよならはるかぜのきせつ 11287). Above this the reading's plain forms lead.
const PLAIN_FORMS_LEAD_COST: i32 = 13_000;
/// Texts remembered per reading (`AppleJapaneseConverter` keeps five).
const LEARNED_PER_READING: usize = 5;

/// The compact dictionary (see tools/build_ipadic.py for the layout).
pub struct KanjiDictionary {
    n_readings: usize,
    n_entries: usize,
    max_reading_chars: usize,
    lsize: usize,
    reading_offsets: &'static [u8],
    entry_offsets: &'static [u8],
    entry_byte_offsets: &'static [u8],
    readings: &'static [u8],
    entries: &'static [u8],
    matrix: &'static [u8],
}

static SHARED: LazyLock<KanjiDictionary> = LazyLock::new(|| KanjiDictionary::new(DICTIONARY));

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn i16_at(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

impl KanjiDictionary {
    /// One dictionary per process, built lazily (Japanese is off by default).
    pub fn shared() -> &'static KanjiDictionary {
        &SHARED
    }

    fn new(data: &'static [u8]) -> Self {
        assert!(
            data.len() > 32 && &data[..8] == MAGIC,
            "data/ipadic_compact.bin: bad magic (rebuild with tools/build_ipadic.py)"
        );
        let u32_at = |at: usize| {
            u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize
        };
        let n_readings = u32_at(8);
        let n_entries = u32_at(12);
        let max_reading_chars = data[16] as usize;
        let (lsize, rsize) = (u16_at(data, 20) as usize, u16_at(data, 22) as usize);
        let (readings_len, entries_len) = (u32_at(24), u32_at(28));
        let tables = 32;
        let table_len = (n_readings + 1) * 4;
        let at_readings = tables + 3 * table_len;
        let at_entries = at_readings + readings_len;
        let at_matrix = at_entries + entries_len;
        assert!(
            at_matrix + lsize * rsize * 2 <= data.len(),
            "data/ipadic_compact.bin: truncated"
        );
        Self {
            n_readings,
            n_entries,
            max_reading_chars,
            lsize,
            reading_offsets: &data[tables..tables + table_len],
            entry_offsets: &data[tables + table_len..tables + 2 * table_len],
            entry_byte_offsets: &data[tables + 2 * table_len..tables + 3 * table_len],
            readings: &data[at_readings..at_readings + readings_len],
            entries: &data[at_entries..at_entries + entries_len],
            matrix: &data[at_matrix..at_matrix + lsize * rsize * 2],
        }
    }

    fn table(table: &[u8], i: usize) -> usize {
        u32::from_le_bytes([
            table[i * 4],
            table[i * 4 + 1],
            table[i * 4 + 2],
            table[i * 4 + 3],
        ]) as usize
    }

    fn reading(&self, i: usize) -> &'static str {
        let blob = self.readings;
        let (a, b) = (
            Self::table(self.reading_offsets, i),
            Self::table(self.reading_offsets, i + 1),
        );
        std::str::from_utf8(&blob[a..b]).expect("readings are UTF-8")
    }

    fn entry_range(&self, i: usize) -> (usize, usize) {
        (
            Self::table(self.entry_offsets, i),
            Self::table(self.entry_offsets, i + 1),
        )
    }

    /// The k-th entry of reading `i`: (left context, right context, word cost, surface).
    fn entry(&self, i: usize, k: usize) -> (u16, u16, i32, &'static str) {
        let blob = self.entries;
        let mut at = Self::table(self.entry_byte_offsets, i);
        let (start, _) = self.entry_range(i);
        for _ in start..k {
            at += 7 + blob[at + 6] as usize;
        }
        let surface_len = blob[at + 6] as usize;
        let surface =
            std::str::from_utf8(&blob[at + 7..at + 7 + surface_len]).expect("surfaces are UTF-8");
        (
            u16_at(blob, at),
            u16_at(blob, at + 2),
            i16_at(blob, at + 4) as i32,
            surface,
        )
    }

    /// The reading's index, if the dictionary has it (readings are sorted).
    pub fn find_reading(&self, key: &str) -> Option<usize> {
        let (mut lo, mut hi) = (0usize, self.n_readings);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.reading(mid).cmp(key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    /// MeCab's transition cost: matrix[left.rcAttr + lsize * right.lcAttr].
    fn conn(&self, left_rc: u16, right_lc: u16) -> i32 {
        let at = (left_rc as usize + self.lsize * right_lc as usize) * 2;
        i16_at(self.matrix, at) as i32
    }

    pub fn reading_count(&self) -> usize {
        self.n_readings
    }

    pub fn entry_count(&self) -> usize {
        self.n_entries
    }
}

/// A word placed in the lattice: one dictionary entry, or one unknown character.
struct Node {
    start: usize,
    end: usize,
    lc: u16,
    rc: u16,
    wcost: i32,
    text: String,
    unknown: bool,
}

impl KanjiDictionary {
    /// The conversion candidates for a reading: the best paths over the whole reading, the
    /// best path's clause boundaries, the user's earlier choices first, then the reading
    /// itself in hiragana and katakana.
    pub fn candidates(&self, kana: &str, learned: Option<&[String]>) -> Vec<ConversionCandidate> {
        if kana.is_empty() {
            return Vec::new();
        }
        let chars: Vec<char> = kana.chars().collect();
        let n = chars.len();

        // The lattice: every entry whose reading matches a span, plus one unknown character
        // per position (so latin runs and stray marks stay as typed).
        let mut nodes: Vec<Node> = Vec::new();
        let mut starts: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut ends: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
        for i in 0..n {
            let max_len = (n - i).min(self.max_reading_chars);
            for len in 1..=max_len {
                let span: String = chars[i..i + len].iter().collect();
                let Some(reading) = self.find_reading(&span) else {
                    continue;
                };
                let (a, b) = self.entry_range(reading);
                for k in a..b {
                    let (lc, rc, wcost, surface) = self.entry(reading, k);
                    nodes.push(Node {
                        start: i,
                        end: i + len,
                        lc,
                        rc,
                        wcost,
                        text: surface.to_string(),
                        unknown: false,
                    });
                    let id = nodes.len() - 1;
                    starts[i].push(id);
                    ends[i + len].push(id);
                }
            }
            nodes.push(Node {
                start: i,
                end: i + 1,
                lc: 0,
                rc: 0,
                wcost: UNKNOWN_COST,
                text: chars[i].to_string(),
                unknown: true,
            });
            let id = nodes.len() - 1;
            starts[i].push(id);
            ends[i + 1].push(id);
        }

        // Backward: the cheapest cost of finishing after each node.
        let mut h = vec![0i32; nodes.len()];
        for pos in (0..=n).rev() {
            for &v in &ends[pos] {
                let node = &nodes[v];
                h[v] = if pos == n {
                    self.conn(node.rc, 0)
                } else {
                    let mut best = i32::MAX;
                    for &w in &starts[pos] {
                        let cost = self.conn(node.rc, nodes[w].lc) + nodes[w].wcost + h[w];
                        best = best.min(cost);
                    }
                    best
                };
            }
        }

        // Forward: the cheapest cost of reaching each node, for the best clauses.
        let mut f = vec![0i32; nodes.len()];
        let mut back: Vec<Option<usize>> = vec![None; nodes.len()];
        for pos in 0..n {
            for &v in &starts[pos] {
                let node = &nodes[v];
                if pos == 0 {
                    f[v] = self.conn(0, node.lc) + node.wcost;
                } else {
                    let mut best = i32::MAX;
                    let mut from = None;
                    for &u in &ends[pos] {
                        let cost = f[u] + self.conn(nodes[u].rc, node.lc) + node.wcost;
                        if cost < best {
                            best = cost;
                            from = Some(u);
                        }
                    }
                    f[v] = best;
                    back[v] = from;
                }
            }
        }
        let mut best_end: Vec<Option<usize>> = vec![None; n + 1];
        for v in 0..nodes.len() {
            let e = nodes[v].end;
            if best_end[e].is_none() || f[v] < f[best_end[e].unwrap()] {
                best_end[e] = Some(v);
            }
        }

        // K best paths over the whole reading (A* with the exact backward costs).
        struct Path {
            parent: Option<usize>,
            node: Option<usize>,
            g: i32,
        }
        let mut arena: Vec<Path> = Vec::new();
        let mut heap: BinaryHeap<Reverse<(i32, usize, usize)>> = BinaryHeap::new();
        let mut seq = 0usize;
        for &v in &starts[0] {
            let g = self.conn(0, nodes[v].lc) + nodes[v].wcost;
            arena.push(Path {
                parent: None,
                node: Some(v),
                g,
            });
            heap.push(Reverse((g + h[v], seq, arena.len() - 1)));
            seq += 1;
        }
        let mut full: Vec<(i32, Vec<usize>)> = Vec::new();
        while let Some(Reverse((_, _, pid))) = heap.pop() {
            let (parent, node, g) = (arena[pid].parent, arena[pid].node, arena[pid].g);
            match node {
                None => {
                    let mut ids = Vec::new();
                    let mut at = parent;
                    while let Some(q) = at {
                        if let Some(v) = arena[q].node {
                            ids.push(v);
                        }
                        at = arena[q].parent;
                    }
                    ids.reverse();
                    full.push((g, ids));
                    if full.len() >= FULL_CANDIDATES {
                        break;
                    }
                }
                Some(v) => {
                    let (end, rc) = (nodes[v].end, nodes[v].rc);
                    if end == n {
                        let total = g + self.conn(rc, 0);
                        arena.push(Path {
                            parent: Some(pid),
                            node: None,
                            g: total,
                        });
                        heap.push(Reverse((total, seq, arena.len() - 1)));
                        seq += 1;
                    } else {
                        for &w in &starts[end] {
                            let ng = g + self.conn(rc, nodes[w].lc) + nodes[w].wcost;
                            arena.push(Path {
                                parent: Some(pid),
                                node: Some(w),
                                g: ng,
                            });
                            heap.push(Reverse((ng + h[w], seq, arena.len() - 1)));
                            seq += 1;
                        }
                    }
                }
            }
        }

        // The best path's clause boundaries, as first-clause candidates.
        let mut prefixes: Vec<(String, usize)> = Vec::new();
        if let Some(last) = best_end[n] {
            let mut boundaries: Vec<usize> = Vec::new();
            let mut at = Some(last);
            while let Some(v) = at {
                if nodes[v].start > 0 {
                    boundaries.push(nodes[v].start);
                }
                at = back[v];
            }
            boundaries.sort_unstable();
            for p in boundaries.into_iter().take(PREFIX_CANDIDATES) {
                if let Some(end) = best_end[p] {
                    let mut parts: Vec<&str> = Vec::new();
                    let mut at = Some(end);
                    while let Some(v) = at {
                        parts.push(nodes[v].text.as_str());
                        at = back[v];
                    }
                    parts.reverse();
                    prefixes.push((parts.concat(), p));
                }
            }
        }

        // A reading no real word covers reads as an odd multi-piece path (よーそろー → よー粗ロー);
        // then the kana forms lead instead of trailing the noise. Unknown characters don't
        // count: a latin run inside a phrase (google) is not a sign of a missing word.
        let plain_forms_lead = match full.first() {
            Some((cost, ids)) => {
                let unknown = ids.iter().filter(|&&v| nodes[v].unknown).count() as i32;
                *cost - unknown * UNKNOWN_COST >= PLAIN_FORMS_LEAD_COST
            }
            None => false,
        };

        let mut out: Vec<ConversionCandidate> = Vec::new();
        if plain_forms_lead {
            out.push(ConversionCandidate::new(kana.to_string(), n));
            out.push(ConversionCandidate::new(RomajiComposer::katakana(kana), n));
        }
        for (_, ids) in &full {
            let text: String = ids.iter().map(|&v| nodes[v].text.as_str()).collect();
            out.push(ConversionCandidate::new(text, n));
        }
        for (text, covered) in prefixes {
            out.push(ConversionCandidate::new(text, covered));
        }
        // The user's earlier choices for this reading first, most recent first.
        if let Some(learned) = learned {
            for text in learned.iter().rev() {
                out.retain(|c| c.text != *text);
                out.insert(0, ConversionCandidate::new(text.clone(), n));
            }
        }
        // The reading itself, in hiragana and katakana, is always available.
        if !plain_forms_lead {
            for plain in [kana.to_string(), RomajiComposer::katakana(kana)] {
                if !out.iter().any(|c| c.text == plain && c.kana_count == n) {
                    out.push(ConversionCandidate::new(plain, n));
                }
            }
        }
        // One row per text (the better-ranked coverage).
        let mut shown: HashSet<String> = HashSet::new();
        out.retain(|c| shown.insert(c.text.clone()));
        out
    }
}

/// The dictionary-backed converter: `candidates` from the lattice, `learn` per covered
/// reading (the same shape as seamtype's `AppleJapaneseConverter`: five texts per reading,
/// most recent first, in `%APPDATA%\Typescript\kanji_learning.json`).
pub struct DictionaryConverter {
    dict: &'static KanjiDictionary,
    learning_path: Option<PathBuf>,
    learned: Mutex<BTreeMap<String, Vec<String>>>,
}

impl DictionaryConverter {
    pub fn new(learning_path: Option<PathBuf>) -> Self {
        let learned = learning_path
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        Self {
            dict: KanjiDictionary::shared(),
            learning_path,
            learned: Mutex::new(learned),
        }
    }

    pub fn default_learning_path() -> Option<PathBuf> {
        support_dir().map(|d| d.join("kanji_learning.json"))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Vec<String>>> {
        self.learned.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl JapaneseConverter for DictionaryConverter {
    fn candidates(&self, kana: &str) -> Vec<ConversionCandidate> {
        let learned = self.lock().get(kana).cloned();
        self.dict.candidates(kana, learned.as_deref())
    }

    fn learn(&self, candidate: &ConversionCandidate, kana: &str) {
        let covered: String = kana.chars().take(candidate.kana_count).collect();
        if covered.is_empty() {
            return;
        }
        let mut learned = self.lock();
        let texts = learned.entry(covered).or_default();
        if texts.first() == Some(&candidate.text) {
            return;
        }
        texts.retain(|t| *t != candidate.text);
        texts.insert(0, candidate.text.clone());
        texts.truncate(LEARNED_PER_READING);
        // Write through, like lexicon.json: failing to save only costs the learning.
        let Some(path) = &self.learning_path else {
            return;
        };
        let Ok(json) = serde_json::to_vec_pretty(&*learned) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".{}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        if fs::write(&tmp, json).is_ok() && fs::rename(&tmp, path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }

    fn save(&self) {
        // Every choice is written on `learn`; nothing to flush.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn converter() -> DictionaryConverter {
        DictionaryConverter::new(None)
    }

    fn texts(candidates: &[ConversionCandidate], first: usize) -> Vec<&str> {
        candidates
            .iter()
            .take(first)
            .map(|c| c.text.as_str())
            .collect()
    }

    #[test]
    fn dictionary_loads() {
        let dict = KanjiDictionary::shared();
        assert!(dict.reading_count() > 150_000);
        assert!(dict.entry_count() > 300_000);
        assert!(dict.find_reading("へんかん").is_some());
        assert!(dict.find_reading("ぬるぽです").is_none());
    }

    #[test]
    fn word_candidates_come_from_the_dictionary() {
        let candidates = converter().candidates("へんかん");
        let top = texts(&candidates, 6);
        assert!(top.contains(&"変換"), "{top:?}");
        assert!(top.contains(&"返還"), "{top:?}");
        // The reading itself is always available, hiragana and katakana.
        let all = texts(&candidates, candidates.len());
        assert!(all.contains(&"へんかん"));
        assert!(all.contains(&"ヘンカン"));
    }

    #[test]
    fn sentence_paths_and_clauses() {
        let candidates = converter().candidates("きょうはいいてんきですね");
        let top = texts(&candidates, 4);
        assert!(
            top.contains(&"今日はいい天気ですね"),
            "no 今日 reading in {top:?}"
        );
        assert!(
            candidate_texts_have_clauses(&candidates, 12),
            "{:?}",
            candidates
                .iter()
                .map(|c| (c.text.clone(), c.kana_count))
                .collect::<Vec<_>>()
        );
    }

    fn candidate_texts_have_clauses(candidates: &[ConversionCandidate], whole: usize) -> bool {
        candidates
            .iter()
            .any(|c| c.kana_count < whole && c.kana_count > 0)
    }

    #[test]
    fn latin_runs_stay_as_typed() {
        let candidates = converter().candidates("きょうはgoogleでけんさく");
        let first = &candidates[0];
        assert!(first.text.contains("google"), "{}", first.text);
        assert_eq!(first.kana_count, "きょうはgoogleでけんさく".chars().count());
        assert_eq!(
            candidates.len(),
            candidates
                .iter()
                .map(|c| &c.text)
                .collect::<HashSet<_>>()
                .len()
        );
    }

    #[test]
    fn learned_choice_comes_first() {
        let path = std::env::temp_dir().join(format!(
            "typescript-kanji-learn-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let converter = DictionaryConverter::new(Some(path.clone()));
        let choice = converter
            .candidates("きょうはいいてんきですね")
            .into_iter()
            .find(|c| c.text == "今日はいい天気ですね")
            .expect("dictionary has the reading");
        converter.learn(&choice, "きょうはいいてんきですね");
        let after = converter.candidates("きょうはいいてんきですね");
        assert_eq!(after[0].text, "今日はいい天気ですね");
        assert_eq!(after[0].kana_count, 12);
        // Persisted: a fresh converter remembers.
        let reloaded = DictionaryConverter::new(Some(path.clone()));
        assert_eq!(
            reloaded.candidates("きょうはいいてんきですね")[0].text,
            "今日はいい天気ですね"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn unknown_reading_falls_back_to_kana() {
        let candidates = converter().candidates("ぬるぽです");
        let all = texts(&candidates, candidates.len());
        assert!(all.contains(&"ぬるぽです"), "{all:?}");
    }

    /// A reading no word covers (a katakana word IPADIC lacks): the plain forms lead instead of
    /// the odd path pieces (よー粗ロー), and a latin run inside a phrase does not count as junk.
    #[test]
    fn plain_forms_lead_when_no_word_covers_the_reading() {
        let candidates = converter().candidates("よーそろー");
        assert_eq!(texts(&candidates, 2), ["よーそろー", "ヨーソロー"]);
        let candidates = converter().candidates("きょうはgoogleでけんさく");
        assert_eq!(candidates[0].text, "今日はgoogleで検索");
    }
}
