//! The Chinese (Mandarin, simplified Pinyin) model: the compact dictionary built by
//! tools/build_pinyin_simp.py (format TSZHPIN1, see its docstring) read zero-copy.
//! `lang::pinyin` splits a typed latin run into syllables; a run that is a dictionary
//! reading is Chinese, and the reading's top word is what the input method shows.
//! The lattice conversion and the candidate window come next; the file already carries
//! every reading's entries in weight order for them.
//!
//! The dictionary is rime-pinyin-simp (Apache-2.0), itself derived from the Android
//! Open Source Project's PinyinIME — see THIRD_PARTY_NOTICES.txt.

use crate::decide::engine::Language;
use crate::lang::pack::{self, PackLazy};
use crate::lang::pinyin;

const MAGIC: &[u8; 8] = b"TSZHPIN1";

/// A full reading's score on the decide scale. It has to lose to a dictionary English
/// word (floors -1.4..-2.6) that shares its letters — `women`, `shanghai`, `fan` — and
/// the engine's English-word protection keeps those English without context; the floor
/// only has to stay above plain English trigram noise.
const WORD_FLOOR: f64 = -1.7;
/// A single syllable is weaker on its own (`ni`, `shi`): collisions with short English
/// words are common, and a lone syllable rarely deserves the word.
const SINGLE_FLOOR: f64 = -2.8;
/// Splits cleanly but is no dictionary reading (syllables picked at random): usually
/// English's noise, so it only wins on context.
const WEAK_SCORE: f64 = -3.9;
/// How far the top dictionary entry lifts the floor: a common word (你好) beats a rare
/// one (哪吒) by this much at most.
const WEIGHT_BONUS: f64 = 0.5;

/// What the engine knows about a run of keys once it reads as Chinese.
pub struct ChineseScore {
    /// Average log-probability per keystroke, comparable with the English and Korean models.
    pub average_log_probability: f64,
    /// The reading the keys split into: syllables joined with single spaces (`ni hao`).
    pub reading: String,
    /// The word to show instead of the reading: the top dictionary entry, and only for a
    /// reading of two or more syllables. A single syllable stays as the reading.
    pub converted: Option<&'static str>,
    /// The whole reading is in the dictionary (any syllable count). The engine treats it
    /// as one more language the keys are "known" in, so a sentence switching into Chinese
    /// doesn't get held back by English's pull.
    pub known: bool,
}

static SHARED: PackLazy<ChineseModel> = PackLazy::new(Language::Chinese, || {
    ChineseModel::new(pack::bytes(Language::Chinese, "pinyin_compact.bin"))
});

/// The compact dictionary (see tools/build_pinyin_simp.py for the packed layout).
/// `walk` refuses anything a reader could trip on — a half-copied file costs the Chinese
/// pack, not the text service — and `new` falls back to an empty model that reads
/// nothing, so calling `shared()` with the pack missing stays harmless.
pub struct ChineseModel {
    /// `n + 1` start offsets into `readings` (byte offsets, prefix sums).
    reading_starts: Vec<u32>,
    /// `n + 1` start offsets into `entries`.
    entry_starts: Vec<u32>,
    readings: &'static [u8],
    entries: &'static [u8],
    max_weight: u32,
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// What a successful walk of the file found (offsets relative to the blob starts).
struct Layout {
    reading_starts: Vec<u32>,
    entry_starts: Vec<u32>,
    readings_len: usize,
    entries_len: usize,
    max_weight: u32,
}

/// The bytes are a whole dictionary the reader can walk: the packed layout, readings
/// sorted, ascii syllables and no stray spaces, entries inside the blob with the counts
/// the header promises. The walk is the one `new` relies on, so a file this accepts
/// cannot panic a lookup.
fn walk(bytes: &[u8]) -> Option<Layout> {
    if bytes.len() < 28 || &bytes[..8] != MAGIC {
        return None;
    }
    let n = u32_at(bytes, 8) as usize;
    let readings_len = u32_at(bytes, 12) as usize;
    let entries_len = u32_at(bytes, 16) as usize;
    let n_entries = u32_at(bytes, 20) as usize;
    if n == 0 {
        return None;
    }
    let arrays_end = 28usize.checked_add(n.checked_mul(8)?)?;
    let readings_end = arrays_end.checked_add(readings_len)?;
    let entries_end = readings_end.checked_add(entries_len)?;
    if entries_end > bytes.len() {
        return None;
    }
    let readings = &bytes[arrays_end..readings_end];
    let entries = &bytes[readings_end..entries_end];

    let mut reading_starts = vec![0u32];
    let mut entry_starts = vec![0u32];
    let mut entry_at = 0usize;
    let mut previous: &[u8] = &[];
    for i in 0..n {
        let len = u16_at(bytes, 28 + 2 * i) as usize;
        let start = *reading_starts.last().unwrap() as usize;
        let reading = readings.get(start..start.checked_add(len)?)?;
        // Ascii syllables joined by single spaces, sorted: ' ' sorts below every letter,
        // so byte order is syllable-sequence order.
        if reading.is_empty()
            || reading[0] == b' '
            || reading[len - 1] == b' '
            || reading.windows(2).any(|w| w[0] == b' ' && w[1] == b' ')
            || !reading.iter().all(|b| b.is_ascii_lowercase() || *b == b' ')
            || reading < previous
        {
            return None;
        }
        previous = reading;
        reading_starts.push((start + len) as u32);

        let count = u16_at(bytes, 28 + 2 * n + 2 * i) as usize;
        let entry_len = u32_at(bytes, 28 + 4 * n + 4 * i) as usize;
        let entry_end = entry_at.checked_add(entry_len)?;
        if count == 0 || entry_end > entries.len() {
            return None;
        }
        let mut at = entry_at;
        for _ in 0..count {
            let word_len = *entries.get(at)? as usize;
            if word_len == 0 {
                return None;
            }
            let word = entries.get(at + 1..at + 1 + word_len)?;
            std::str::from_utf8(word).ok()?;
            if at + 1 + word_len + 4 > entry_end {
                return None;
            }
            at += 1 + word_len + 4;
        }
        if at != entry_end {
            return None;
        }
        entry_starts.push(entry_end as u32);
        entry_at = entry_end;
    }
    if entry_at != entries_len {
        return None;
    }
    let sum: usize = (0..n)
        .map(|i| u16_at(bytes, 28 + 2 * n + 2 * i) as usize)
        .sum();
    if sum != n_entries {
        return None;
    }
    let mut max_weight = 0u32;
    for pair in entry_starts.windows(2) {
        let mut at = pair[0] as usize;
        while at < pair[1] as usize {
            let word_len = entries[at] as usize;
            max_weight = max_weight.max(u32_at(entries, at + 1 + word_len));
            at += 1 + word_len + 4;
        }
    }
    Some(Layout {
        reading_starts,
        entry_starts,
        readings_len,
        entries_len,
        max_weight,
    })
}

impl ChineseModel {
    /// One model per process, built lazily (Chinese is off by default).
    pub fn shared() -> &'static ChineseModel {
        SHARED.get()
    }

    /// The bytes are a table this model can be built from (what `walk` checks).
    pub fn valid(bytes: &[u8]) -> bool {
        walk(bytes).is_some()
    }

    fn empty() -> Self {
        ChineseModel {
            reading_starts: vec![0],
            entry_starts: vec![0],
            readings: &[],
            entries: &[],
            max_weight: 0,
        }
    }

    fn new(bytes: &'static [u8]) -> Self {
        let Some(layout) = walk(bytes) else {
            return Self::empty();
        };
        let n = layout.reading_starts.len() - 1; // starts carry n + 1 prefix sums
        let arrays_end = 28 + n * 8;
        let readings = &bytes[arrays_end..arrays_end + layout.readings_len];
        let entries = &bytes[arrays_end + layout.readings_len..][..layout.entries_len];
        ChineseModel {
            reading_starts: layout.reading_starts,
            entry_starts: layout.entry_starts,
            readings,
            entries,
            max_weight: layout.max_weight,
        }
    }

    fn reading_bytes(&self, i: usize) -> &[u8] {
        let readings: &'static [u8] = self.readings;
        &readings[self.reading_starts[i] as usize..self.reading_starts[i + 1] as usize]
    }

    /// The reading's position in the sorted index, if it is there.
    fn find(&self, reading: &str) -> Option<usize> {
        let key = reading.as_bytes();
        let (mut lo, mut hi) = (0usize, self.reading_starts.len() - 1);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.reading_bytes(mid).cmp(key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    /// The reading's best entry (the entries are stored by weight, descending).
    fn best(&self, i: usize) -> Option<(&'static str, u32)> {
        let entries: &'static [u8] = self.entries;
        let at = self.entry_starts[i] as usize;
        if at >= self.entry_starts[i + 1] as usize {
            return None;
        }
        let word_len = self.entries[at] as usize;
        let word = std::str::from_utf8(&entries[at + 1..at + 1 + word_len]).ok()?;
        Some((word, u32_at(self.entries, at + 1 + word_len)))
    }

    /// How Chinese `keys` reads: `None` when they are no pinyin at all. A reading whose
    /// word is in the dictionary scores near the word floors; a clean split with no word
    /// stays weak, so English wins it without context.
    pub fn score(&self, keys: &str, _partial: bool) -> Option<ChineseScore> {
        if self.reading_starts.len() <= 1 {
            return None; // no dictionary loaded: nothing reads as Chinese
        }
        let syllables = pinyin::split(keys)?;
        let reading = syllables.join(" ");
        let full = self.find(&reading).and_then(|i| self.best(i));
        let average_log_probability = match full {
            Some((_, weight)) => {
                let floor = if syllables.len() >= 2 {
                    WORD_FLOOR
                } else {
                    SINGLE_FLOOR
                };
                let top = (f64::from(self.max_weight) + 1.0).ln();
                floor + WEIGHT_BONUS * ((f64::from(weight) + 1.0).ln() / top).min(1.0)
            }
            None => WEAK_SCORE,
        };
        Some(ChineseScore {
            average_log_probability,
            reading,
            converted: full.filter(|_| syllables.len() >= 2).map(|(word, _)| word),
            known: full.is_some(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packs a tiny dictionary the way tools/build_pinyin_simp.py does.
    fn packed(readings: &[(&str, &[(&str, u32)])]) -> Vec<u8> {
        let (mut reading_lens, mut counts, mut entry_lens) = (Vec::new(), Vec::new(), Vec::new());
        let (mut reading_blob, mut entry_blob) = (Vec::new(), Vec::new());
        for (reading, entries) in readings {
            reading_lens.push(reading.len() as u16);
            reading_blob.extend_from_slice(reading.as_bytes());
            let mut count = 0u16;
            let mut len = 0u32;
            for (word, weight) in *entries {
                entry_blob.push(word.len() as u8);
                entry_blob.extend_from_slice(word.as_bytes());
                entry_blob.extend_from_slice(&weight.to_le_bytes());
                len += 1 + word.len() as u32 + 4;
                count += 1;
            }
            counts.push(count);
            entry_lens.push(len);
        }
        let mut out = Vec::new();
        out.extend(MAGIC);
        out.extend((readings.len() as u32).to_le_bytes());
        out.extend((reading_blob.len() as u32).to_le_bytes());
        out.extend((entry_blob.len() as u32).to_le_bytes());
        out.extend(
            counts
                .iter()
                .map(|c| u32::from(*c))
                .sum::<u32>()
                .to_le_bytes(),
        );
        out.extend(0u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        for len in &reading_lens {
            out.extend(len.to_le_bytes());
        }
        for c in &counts {
            out.extend(c.to_le_bytes());
        }
        for len in &entry_lens {
            out.extend(len.to_le_bytes());
        }
        out.extend_from_slice(&reading_blob);
        out.extend_from_slice(&entry_blob);
        out
    }

    #[test]
    fn a_table_the_model_cannot_read_is_refused() {
        let good = packed(&[("ni", &[("你", 9)]), ("ni hao", &[("你好", 7)])]);
        assert!(ChineseModel::valid(&good));
        let blob = 28 + 2 * 2 + 2 * 2 + 4 * 2;
        for (what, bytes) in [
            ("empty", Vec::new()),
            ("a bad magic", {
                let mut b = good.clone();
                b[0] = b'X';
                b
            }),
            (
                "a record that is cut short",
                good[..good.len() - 1].to_vec(),
            ),
            ("a zero count", {
                let mut b = good.clone();
                b[28 + 2 * 2] = 0; // first entry count
                b[28 + 2 * 2 + 1] = 0;
                b
            }),
            ("an unsorted reading", {
                let mut b = good.clone();
                b[blob] = b'z'; // "ni" -> "zi", now above "ni hao"
                b
            }),
            ("a stray space", {
                let mut b = good.clone();
                b[blob + 2] = b' '; // "ni" -> "n ", a trailing space
                b
            }),
        ] {
            assert!(!ChineseModel::valid(&bytes), "{what} must fail valid()");
        }
    }

    #[test]
    fn loads_and_scores() {
        let m = ChineseModel::shared();
        let word = m.score("nihao", false).expect("splits");
        assert_eq!(word.reading, "ni hao");
        assert_eq!(word.converted, Some("你好"));
        assert_eq!(
            m.score("nihaoma", false).and_then(|s| s.converted),
            Some("你好吗")
        );
        let single = m.score("ni", false).expect("splits");
        assert_eq!(single.converted, None);
        assert!(single.average_log_probability < word.average_log_probability);
        // No pinyin at all: not Chinese.
        assert!(m.score("hello", false).is_none());
        assert!(m.score("dkssud", false).is_none());
        assert!(m.score("", false).is_none());
        assert!(m.score("ni3", false).is_none());
    }
}
