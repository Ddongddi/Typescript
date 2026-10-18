//! Native kana → kanji conversion over Mozc's OSS lexicon and IME connection costs.
//! The Rust lattice, candidate UI and learning remain Typescript's; no Mozc runtime is linked.
//! Readings use a sorted hiragana index, word paths use Viterbi and A* N-best search.
//!
//! `tools/build_mozc.py` builds `data/ipadic_compact.bin` (legacy filename) as TSIPDIC4:
//! 12-bit contexts, u16 homophone counts and full Unicode scalars. TSIPDIC3 IPADIC remains readable.
//! The blobs are still read zero-copy; the stored per-reading lengths become u32 prefix sums
//! once, at load.
//! The same dictionary and converter serve every platform through JapaneseConverter.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use crate::decide::convert::{ConversionCandidate, JapaneseConverter};
use crate::decide::engine::Language;
use crate::lang::pack;
use crate::lang::romaji::RomajiComposer;
use crate::support_dir;

const MAGIC: &[u8; 8] = b"TSIPDIC3";
const WIDE_MAGIC: &[u8; 8] = b"TSIPDIC4";

/// No entry covers this character (a latin run, a stray mark): the path keeps it as typed.
const UNKNOWN_COST: i32 = 15_000;
/// Whole-reading paths offered, best first.
const FULL_CANDIDATES: usize = 8;
// shortcut: bound N-best memory; use a lazy k-shortest-path algorithm if this clips useful alternatives.
const MAX_NBEST_STATES: usize = 65_536;
/// Clause boundaries of the best path, offered as first-clause candidates.
const PREFIX_CANDIDATES: usize = 4;
/// A reading no real word covers comes out as an odd multi-piece path (よーそろー → よー粗ロー at
/// 14124, きょうはよーそろー at 17168); readings that do have words, latin runs and phrases with
/// their unknown cost removed stay under 12000 in measurements (きょうはgoogleでけんさく 9019,
/// さよならはるかぜのきせつ 11287). Above this the reading's plain forms lead.
const PLAIN_FORMS_LEAD_COST: i32 = 13_000;
/// Texts remembered per reading (`AppleJapaneseConverter` keeps five).
const LEARNED_PER_READING: usize = 5;

/// The compact dictionary (see tools/build_ipadic.py for the packed layout, and
/// tools/repack_ipadic.py for upgrading an older file).
pub struct KanjiDictionary {
    wide: bool,
    n_readings: usize,
    n_entries: usize,
    max_reading_chars: usize,
    lsize: usize,
    reading_offsets: Vec<u32>,
    entry_byte_offsets: Vec<u32>,
    readings: &'static [u8],
    entries: &'static [u8],
    top_chars: &'static [u8],
    rest_chars: &'static [u8],
    matrix: &'static [u8],
}

static SHARED: LazyLock<KanjiDictionary> =
    LazyLock::new(|| KanjiDictionary::new(pack::bytes(Language::Japanese, "ipadic_compact.bin")));

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn i16_at(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u24_at(bytes: &[u8], at: usize) -> usize {
    bytes[at] as usize | ((bytes[at + 1] as usize) << 8) | ((bytes[at + 2] as usize) << 16)
}

/// A reading as the file stores it: one byte per character, in an order the byte compare keeps
/// (ぁ..ゖ -> 0..85, ー -> 86). `None` when a character cannot appear in a reading: no entry can
/// match, so the search can answer without comparing.
fn encoded_reading(key: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(key.len());
    for c in key.chars() {
        let v = c as u32;
        if (0x3041..=0x3096).contains(&v) {
            out.push((v - 0x3041) as u8);
        } else if v == 0x30FC {
            out.push(86);
        } else {
            return None;
        }
    }
    Some(out)
}

impl KanjiDictionary {
    /// One dictionary per process, built lazily (Japanese is off by default).
    pub fn shared() -> &'static KanjiDictionary {
        &SHARED
    }

    /// The bytes are a whole dictionary the reader can walk: right magic, tables that fit the
    /// file, and, the part a damaged file would otherwise hit the first time a word is
    /// converted, lengths, codes and ids that stay inside those tables. The walk below is the
    /// same one `candidates` does, so a file this accepts cannot panic it. A pack is only
    /// registered when this holds, so a half-copied file costs the Japanese pack, not the
    /// text service.
    ///
    /// The arithmetic runs on file-supplied numbers: all of it checked, so a crafted header
    /// cannot wrap (the shipped 32-bit service is where that would bite).
    pub fn valid(data: &[u8]) -> bool {
        if data.len() <= 36 || (&data[..8] != MAGIC && &data[..8] != WIDE_MAGIC) {
            return false;
        }
        let wide = &data[..8] == WIDE_MAGIC;
        let count_width = if wide { 2 } else { 1 };
        let char_width = if wide { 4 } else { 2 };
        let bits = if wide { 12 } else { 11 };
        let u32_at = |at: usize| {
            u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize
        };
        let (lsize, rsize) = (u16_at(data, 20) as usize, u16_at(data, 22) as usize);
        // Every node has one connection to the matrix, even a file with no entries: the two
        // sizes have to hold.
        if lsize == 0 || rsize == 0 {
            return false;
        }
        let n_readings = u32_at(8);
        let (readings_len, entries_len) = (u32_at(24), u32_at(28));
        let (n_top, n_rest) = (u16_at(data, 32) as usize, u16_at(data, 34) as usize);
        // One-byte surface codes index the top table, two-byte codes the rest; the reader
        // takes both as sizes, so they have to hold.
        if n_top > 128 || n_rest > 0x8000 {
            return false;
        }
        let Some(at_charset) = n_readings
            .checked_mul(3 + count_width)
            .and_then(|n| n.checked_add(36))
        else {
            return false;
        };
        let Some(at_readings) = n_top
            .checked_add(n_rest)
            .and_then(|n| n.checked_mul(char_width))
            .and_then(|n| n.checked_add(at_charset))
        else {
            return false;
        };
        let Some(regions_end) = at_readings
            .checked_add(readings_len)
            .and_then(|n| n.checked_add(entries_len))
        else {
            return false;
        };
        let Some(matrix_bytes) = lsize.checked_mul(rsize).and_then(|n| n.checked_mul(2)) else {
            return false;
        };
        if regions_end
            .checked_add(matrix_bytes)
            .is_none_or(|end| end > data.len())
        {
            return false;
        }
        let tables = 36;
        let reading_lens = &data[tables..tables + n_readings];
        let entry_counts = &data[tables + n_readings..tables + (1 + count_width) * n_readings];
        let entry_byte_lens = &data[tables + (1 + count_width) * n_readings..at_charset];
        let charset = &data[at_charset..at_readings];
        let readings = &data[at_readings..at_readings + readings_len];
        let entries = &data[at_readings + readings_len..regions_end];
        // Every code point has to be a character; the reader takes them as such.
        for i in 0..(n_top + n_rest) {
            let code = if wide {
                let at = i * 4;
                u32::from_le_bytes(charset[at..at + 4].try_into().unwrap())
            } else {
                u16_at(charset, i * 2) as u32
            };
            if char::from_u32(code).is_none() {
                return false;
            }
        }
        // Readings: one byte per character, every byte a kana code, the lengths not adding
        // past the blob.
        let mut at = 0usize;
        for &len in reading_lens {
            let Some(next) = at.checked_add(len as usize) else {
                return false;
            };
            if next > readings_len || readings[at..next].iter().any(|&b| b > 86) {
                return false;
            }
            at = next;
        }
        // Entries: walk every reading's entries exactly as the reader will, and check that the
        // per-reading byte lengths the reader jumps by agree with the records it will find.
        let mut at = 0usize;
        let mut walked = 0usize;
        for i in 0..n_readings {
            let start = at;
            let count = if wide {
                u16_at(entry_counts, i * 2) as usize
            } else {
                entry_counts[i] as usize
            };
            let Some(total) = walked.checked_add(count) else {
                return false;
            };
            // Each record is 6 header bytes plus its surface, so more entries than that in
            // bytes is a lie (and keeps this walk linear).
            if total > entries_len / 6 {
                return false;
            }
            walked = total;
            for _ in 0..count {
                if at + 6 > entries_len {
                    return false;
                }
                let packed = u24_at(entries, at);
                if (packed & ((1 << bits) - 1)) >= rsize || (packed >> bits) >= lsize {
                    return false;
                }
                let end = at + 6 + entries[at + 5] as usize;
                if end > entries_len {
                    return false;
                }
                let mut p = at + 6;
                while p < end {
                    let code = entries[p];
                    if code < 0x80 {
                        if (code as usize) >= n_top {
                            return false;
                        }
                        p += 1;
                    } else {
                        if p + 2 > end {
                            return false;
                        }
                        let index = (((code - 0x80) as usize) << 8) | entries[p + 1] as usize;
                        if index >= n_rest {
                            return false;
                        }
                        p += 2;
                    }
                }
                at = end;
            }
            if at - start != u16_at(entry_byte_lens, i * 2) as usize {
                return false;
            }
        }
        true
    }

    fn new(data: &'static [u8]) -> Self {
        assert!(
            Self::valid(data),
            "ipadic_compact.bin: invalid dictionary (rebuild with tools/build_mozc.py)"
        );
        let u32_at = |at: usize| {
            u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize
        };
        let n_readings = u32_at(8);
        let wide = &data[..8] == WIDE_MAGIC;
        let count_width = if wide { 2 } else { 1 };
        let char_width = if wide { 4 } else { 2 };
        let n_entries = u32_at(12);
        let max_reading_chars = data[16] as usize;
        let (lsize, rsize) = (u16_at(data, 20) as usize, u16_at(data, 22) as usize);
        let (readings_len, entries_len) = (u32_at(24), u32_at(28));
        let (n_top, n_rest) = (u16_at(data, 32) as usize, u16_at(data, 34) as usize);
        let tables = 36;
        let at_charset = tables + (3 + count_width) * n_readings;
        let at_readings = at_charset + char_width * (n_top + n_rest);
        let at_entries = at_readings + readings_len;
        let at_matrix = at_entries + entries_len;
        assert!(
            at_matrix + lsize * rsize * 2 <= data.len(),
            "data/ipadic_compact.bin: truncated"
        );
        // The prefix sums the reader jumps by, widened from the stored lengths once.
        let mut reading_offsets = Vec::with_capacity(n_readings + 1);
        let mut entry_byte_offsets = Vec::with_capacity(n_readings + 1);
        reading_offsets.push(0);
        entry_byte_offsets.push(0);
        for i in 0..n_readings {
            reading_offsets.push(reading_offsets[i] + data[tables + i] as u32);
            entry_byte_offsets.push(
                entry_byte_offsets[i]
                    + u16_at(data, tables + (1 + count_width) * n_readings + 2 * i) as u32,
            );
        }
        Self {
            wide,
            n_readings,
            n_entries,
            max_reading_chars,
            lsize,
            reading_offsets,
            entry_byte_offsets,
            readings: &data[at_readings..at_readings + readings_len],
            entries: &data[at_entries..at_entries + entries_len],
            top_chars: &data[at_charset..at_charset + char_width * n_top],
            rest_chars: &data
                [at_charset + char_width * n_top..at_charset + char_width * (n_top + n_rest)],
            matrix: &data[at_matrix..at_matrix + lsize * rsize * 2],
        }
    }

    /// The i-th reading as stored (one byte per character).
    fn reading(&self, i: usize) -> &'static [u8] {
        let blob = self.readings;
        &blob[self.reading_offsets[i] as usize..self.reading_offsets[i + 1] as usize]
    }

    fn top_char(&self, code: u8) -> char {
        self.surface_char(self.top_chars, code as usize)
    }

    fn surface_char(&self, chars: &[u8], index: usize) -> char {
        let code = if self.wide {
            u32::from_le_bytes(chars[index * 4..index * 4 + 4].try_into().unwrap())
        } else {
            u16_at(chars, index * 2) as u32
        };
        char::from_u32(code).expect("charset code points are characters")
    }

    fn rest_char(&self, index: usize) -> char {
        self.surface_char(self.rest_chars, index)
    }

    /// Decode each entry once; rescanning from the start made common readings quadratic.
    fn entries(&self, i: usize) -> impl Iterator<Item = (u16, u16, i32, String)> + '_ {
        let mut at = self.entry_byte_offsets[i] as usize;
        let end = self.entry_byte_offsets[i + 1] as usize;
        std::iter::from_fn(move || {
            if at >= end {
                return None;
            }
            let value = self.entry(at);
            at += 6 + self.entries[at + 5] as usize;
            Some(value)
        })
    }

    fn entry(&self, at: usize) -> (u16, u16, i32, String) {
        let blob = self.entries;
        let packed = u24_at(blob, at);
        let bits = if self.wide { 12 } else { 11 };
        let nbytes = blob[at + 5] as usize;
        let mut text = String::with_capacity(nbytes);
        let mut p = at + 6;
        let end = p + nbytes;
        while p < end {
            let code = blob[p];
            if code < 0x80 {
                text.push(self.top_char(code));
                p += 1;
            } else {
                text.push(self.rest_char((((code - 0x80) as usize) << 8) | blob[p + 1] as usize));
                p += 2;
            }
        }
        (
            (packed & ((1 << bits) - 1)) as u16,
            (packed >> bits) as u16,
            i16_at(blob, at + 3) as i32,
            text,
        )
    }

    /// The reading's index, if the dictionary has it (readings are sorted, and the one-byte
    /// encoding preserves the order, so the search compares bytes).
    pub fn find_reading(&self, key: &str) -> Option<usize> {
        let key = encoded_reading(key)?;
        self.find_encoded(&key)
    }

    fn find_encoded(&self, key: &[u8]) -> Option<usize> {
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
        // Encode once, rather than allocating and encoding every possible substring.
        let codes: Vec<u8> = chars
            .iter()
            .map(|&c| match c {
                '\u{3041}'..='\u{3096}' => (c as u32 - 0x3041) as u8,
                'ー' => 86,
                _ => 255,
            })
            .collect();

        // The lattice: every entry whose reading matches a span, plus one unknown character
        // per position (so latin runs and stray marks stay as typed).
        let mut nodes: Vec<Node> = Vec::new();
        let mut starts: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut ends: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
        for i in 0..n {
            let max_len = (n - i).min(self.max_reading_chars);
            for len in 1..=max_len {
                if codes[i + len - 1] == 255 {
                    break;
                }
                let Some(reading) = self.find_encoded(&codes[i..i + len]) else {
                    continue;
                };
                for (lc, rc, wcost, surface) in self.entries(reading) {
                    nodes.push(Node {
                        start: i,
                        end: i + len,
                        lc,
                        rc,
                        wcost,
                        text: surface,
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
            text: Rc<str>,
        }
        let mut arena: Vec<Path> = Vec::new();
        let mut heap: BinaryHeap<Reverse<(i32, usize, usize)>> = BinaryHeap::new();
        let mut seq = 0usize;
        // Equal output prefixes with the same right context have identical continuations.
        let mut states: HashMap<(usize, u16, Rc<str>), i32> = HashMap::new();
        for &v in &starts[0] {
            let g = self.conn(0, nodes[v].lc) + nodes[v].wcost;
            let text: Rc<str> = nodes[v].text.as_str().into();
            let key = (nodes[v].end, nodes[v].rc, text.clone());
            if states.get(&key).is_some_and(|&cost| cost <= g) {
                continue;
            }
            states.insert(key, g);
            arena.push(Path {
                parent: None,
                node: Some(v),
                g,
                text,
            });
            heap.push(Reverse((g + h[v], seq, arena.len() - 1)));
            seq += 1;
        }
        let mut full: Vec<(i32, Vec<usize>)> = Vec::new();
        let mut full_texts = HashSet::new();
        'search: while let Some(Reverse((_, _, pid))) = heap.pop() {
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
                    if !full_texts.insert(arena[pid].text.clone()) {
                        continue;
                    }
                    full.push((g, ids));
                    if full.len() >= FULL_CANDIDATES {
                        break;
                    }
                }
                Some(v) => {
                    let (end, rc) = (nodes[v].end, nodes[v].rc);
                    if states.get(&(end, rc, arena[pid].text.clone())) != Some(&g) {
                        continue;
                    }
                    if end == n {
                        let total = g + self.conn(rc, 0);
                        arena.push(Path {
                            parent: Some(pid),
                            node: None,
                            g: total,
                            text: arena[pid].text.clone(),
                        });
                        heap.push(Reverse((total, seq, arena.len() - 1)));
                        seq += 1;
                    } else {
                        for &w in &starts[end] {
                            if arena.len() >= MAX_NBEST_STATES {
                                break 'search;
                            }
                            let ng = g + self.conn(rc, nodes[w].lc) + nodes[w].wcost;
                            let text: Rc<str> =
                                format!("{}{}", arena[pid].text, nodes[w].text).into();
                            let key = (nodes[w].end, nodes[w].rc, text.clone());
                            if states.get(&key).is_some_and(|&cost| cost <= ng) {
                                continue;
                            }
                            states.insert(key, ng);
                            arena.push(Path {
                                parent: Some(pid),
                                node: Some(w),
                                g: ng,
                                text,
                            });
                            heap.push(Reverse((ng + h[w], seq, arena.len() - 1)));
                            seq += 1;
                        }
                    }
                }
            }
        }

        // The Viterbi optimum remains available even if a degenerate lattice exhausts N-best.
        if full.is_empty() {
            if let Some(&last) = ends[n]
                .iter()
                .min_by_key(|&&v| f[v] + self.conn(nodes[v].rc, 0))
            {
                let cost = f[last] + self.conn(nodes[last].rc, 0);
                let mut ids = Vec::new();
                let mut at = Some(last);
                while let Some(v) = at {
                    ids.push(v);
                    at = back[v];
                }
                ids.reverse();
                full.push((cost, ids));
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
        // IPADIC's absolute cost threshold is not calibrated for Mozc's IME cost model.
        let plain_forms_lead = !self.wide
            && match full.first() {
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
        for (_cost, ids) in &full {
            let text: String = ids.iter().map(|&v| nodes[v].text.as_str()).collect();
            #[cfg(feature = "tuning")]
            if std::env::var_os("TS_JA_TRACE_COSTS").is_some() {
                eprintln!("{kana}\t{_cost}\t{text}");
            }
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
        let learned = crate::store::load_or_quarantine(learning_path.as_deref());
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
        let _ = crate::store::persist(path, &*learned);
    }

    fn save(&self) {
        // Every choice is written on `learn`; nothing to flush.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
    fn duplicate_paths_do_not_spend_the_whole_candidate_budget() {
        let kana = "こうつうりょうだってそっちのほうがおおい";
        let candidates = converter().candidates(kana);
        let whole: Vec<_> = candidates
            .iter()
            .filter(|c| c.kana_count == kana.chars().count())
            .collect();
        assert!(whole.len() >= FULL_CANDIDATES, "{whole:?}");
        assert_eq!(whole[0].text, "交通量だってそっちのほうが多い");
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
    fn known_katakana_words_do_not_fragment() {
        let candidates = converter().candidates("よーそろー");
        assert_eq!(candidates[0].text, "ヨーソロー");
        assert!(candidates.iter().any(|c| c.text == "よーそろー"));
        let candidates = converter().candidates("きょうはgoogleでけんさく");
        assert_eq!(candidates[0].text, "今日はgoogleで検索");
    }

    /// A whole, minimal dictionary — two readings, three entries — small enough to damage one
    /// field at a time, in the layout tools/build_ipadic.py writes.
    fn minimal_dictionary(wide: bool) -> Vec<u8> {
        let readings = ["あ", "い"]; // codes 0 and 1
        let charset = [if wide { '𠮷' } else { 'か' }, 'き', 'カ'];
        // (reading, lc, rc, cost, surface code)
        let entries: [(usize, u16, u16, i16, u8); 3] = [
            (0, if wide { 2048 } else { 1 }, 1, 100, 0),
            (0, 0, 2, 50, 1),
            (1, 1, 1, 10, 2),
        ];
        let (lsize, rsize) = (3u16, if wide { 2049u16 } else { 2u16 });

        let mut readings_blob = Vec::new();
        let mut reading_lens = Vec::new();
        for r in readings {
            let code = encoded_reading(r).unwrap();
            reading_lens.push(code.len() as u8);
            readings_blob.extend(code);
        }
        let mut entries_blob = Vec::new();
        let mut entry_counts = Vec::new();
        let mut entry_byte_lens = Vec::new();
        for i in 0..readings.len() {
            let start = entries_blob.len();
            let mut n = 0u8;
            for (reading, lc, rc, cost, surface) in entries {
                if reading != i {
                    continue;
                }
                let packed = u32::from(lc) | (u32::from(rc) << if wide { 12 } else { 11 });
                entries_blob.extend(&packed.to_le_bytes()[..3]);
                entries_blob.extend(cost.to_le_bytes());
                entries_blob.push(1); // one surface code byte
                entries_blob.push(surface);
                n += 1;
            }
            if wide {
                entry_counts.extend((n as u16).to_le_bytes());
            } else {
                entry_counts.push(n);
            }
            entry_byte_lens.extend(((entries_blob.len() - start) as u16).to_le_bytes());
        }

        let mut out = Vec::new();
        out.extend(if wide { WIDE_MAGIC } else { MAGIC });
        out.extend(2u32.to_le_bytes()); // readings
        out.extend(3u32.to_le_bytes()); // entries
        out.push(1); // longest reading, in characters
        out.extend([0u8; 3]); // padding
        out.extend(lsize.to_le_bytes());
        out.extend(rsize.to_le_bytes());
        out.extend((readings_blob.len() as u32).to_le_bytes());
        out.extend((entries_blob.len() as u32).to_le_bytes());
        out.extend(3u16.to_le_bytes()); // charset: three one-byte characters
        out.extend(0u16.to_le_bytes()); // ...and no two-byte ones
        out.extend(reading_lens);
        out.extend(entry_counts);
        out.extend(entry_byte_lens);
        for c in charset {
            if wide {
                out.extend((c as u32).to_le_bytes());
            } else {
                out.extend((c as u16).to_le_bytes());
            }
        }
        out.extend(readings_blob);
        out.extend(entries_blob);
        for _ in 0..(lsize as usize * rsize as usize) {
            out.extend(0i16.to_le_bytes()); // the connection matrix
        }
        out
    }

    fn patched(dictionary: &[u8], at: usize, bytes: &[u8]) -> Vec<u8> {
        let mut out = dictionary.to_vec();
        out[at..at + bytes.len()].copy_from_slice(bytes);
        out
    }

    /// A damaged dictionary is refused by `valid` in every place the reader would otherwise
    /// walk into it — a length past a blob, a code outside a table, an id outside the matrix
    /// (this is the check that keeps a half-copied pack from panicking the text service).
    #[test]
    fn a_damaged_dictionary_fails_valid_where_the_reader_would_walk() {
        let whole = minimal_dictionary(false);
        assert!(
            KanjiDictionary::valid(&whole),
            "the fixture itself must pass"
        );
        // Where each field sits in the fixture above.
        let (reading_lens_at, entry_counts_at, entry_byte_lens_at) = (36, 38, 40);
        let (charset_at, readings_at, entries_at) = (44, 50, 52);
        let cases: [(&str, Vec<u8>); 10] = [
            (
                "a reading length past the readings blob",
                patched(&whole, reading_lens_at, &[200]),
            ),
            (
                "a reading byte that is not a kana code",
                patched(&whole, readings_at, &[0xFF]),
            ),
            (
                "a left id outside the matrix",
                patched(&whole, entries_at, &[0x02, 0x08, 0x00]),
            ),
            (
                "a right id outside the matrix",
                patched(&whole, entries_at, &[0x01, 0x18, 0x00]),
            ),
            (
                "a surface code that indexes past the tables",
                patched(&whole, entries_at + 6, &[0x80]),
            ),
            (
                "a surface running past the entries blob",
                patched(&whole, entries_at + 19, &[200]),
            ),
            (
                "more entries claimed than the blob can hold",
                patched(&whole, entry_counts_at, &[250]),
            ),
            (
                "a byte length that disagrees with the records",
                patched(&whole, entry_byte_lens_at, &[15, 0]),
            ),
            (
                "a code point that is not a character",
                patched(&whole, charset_at, &[0x00, 0xD8]),
            ),
            (
                "a zero matrix width",
                patched(&whole, 20, &0u16.to_le_bytes()),
            ),
        ];
        for (what, bytes) in cases {
            assert!(!KanjiDictionary::valid(&bytes), "{what} must fail valid()");
        }
    }

    #[test]
    fn wide_dictionary_reads_contexts_counts_and_unicode_and_rejects_damage() {
        let bytes = minimal_dictionary(true);
        assert!(KanjiDictionary::valid(&bytes));
        // Two reading lengths, two u16 counts, two u16 lengths, then three u32 scalars.
        assert!(!KanjiDictionary::valid(&patched(
            &bytes,
            38,
            &300u16.to_le_bytes()
        )));
        assert!(!KanjiDictionary::valid(&patched(
            &bytes,
            46,
            &0x110000u32.to_le_bytes()
        )));
        for length in [0, 35, bytes.len() - 1] {
            assert!(!KanjiDictionary::valid(&bytes[..length]));
        }
        let dict = KanjiDictionary::new(Box::leak(bytes.into_boxed_slice()));
        let entries: Vec<_> = dict.entries(dict.find_reading("あ").unwrap()).collect();
        assert_eq!(
            entries,
            [(2048, 1, 100, "𠮷".into()), (0, 2, 50, "き".into())]
        );
    }
}
