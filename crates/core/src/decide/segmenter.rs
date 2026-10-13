//! Finds English words typed inside a Japanese phrase, the way Japanese is written:
//! `kyouhagoogledekensaku` → きょうは + google + でけんさく.
//!
//! Only used when the whole phrase doesn't compose to clean kana (google → ごおgぇ leaves
//! a stray "g"). Splits the romaji into clean Japanese spans and known English words
//! (3+ letters), maximizing the Japanese model's score plus a fixed score per English
//! word, with a cost per switch so ordinary Japanese is never carved up.
//!
//! The input method asks on every keystroke, so the search only cuts where an English
//! word could start or end (no English word anywhere → nothing to do), and the last
//! answer is remembered.
//! Port of JapaneseSegmenter.swift.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};

use crate::lang::english::EnglishModel;
use crate::lang::japanese::JapaneseModel;
use crate::lang::romaji::RomajiComposer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Japanese { keys: String, kana: String },
    English(String),
}

/// Longest English word looked for inside a phrase.
const MAX_ENGLISH_LENGTH: usize = 20;

static PARTICLES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "で", "に", "を", "は", "が", "の", "と", "も", "へ", "や", "か", "ね", "よ",
    ]
    .into_iter()
    .collect()
});

/// Case particles that may follow an English noun on their own: githubで, slackから.
/// Not ね/よ/か/や/へ/って: names end in them far more often (jayne, rebecka, claudette).
static TRAILING_PARTICLES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "で", "に", "を", "は", "が", "の", "と", "も", "から", "まで", "より",
    ]
    .into_iter()
    .collect()
});

/// A segment path with its total log-probability.
type ScoredSegments = (Vec<Segment>, f64);
/// The last few answers, newest first: `(keys, answer)`.
type SegmentCache = Arc<Mutex<Vec<(String, Option<ScoredSegments>)>>>;

struct State {
    score: f64,
    back: Option<(usize, usize, bool)>,
    segment: Option<Segment>,
}

impl Clone for State {
    fn clone(&self) -> Self {
        Self {
            score: self.score,
            back: self.back,
            segment: self.segment.clone(),
        }
    }
}

pub struct JapaneseSegmenter {
    pub english: &'static EnglishModel,
    pub japanese: &'static JapaneseModel,
    /// Log-probability credited per letter of a known English word.
    pub english_letter_log_p: f64,
    /// Cost of each switch into English.
    pub switch_cost: f64,
    /// The last few answers: the preview, the decision and the commit ask about the same keys.
    cache: SegmentCache,
}

impl Clone for JapaneseSegmenter {
    fn clone(&self) -> Self {
        Self {
            english: self.english,
            japanese: self.japanese,
            english_letter_log_p: self.english_letter_log_p,
            switch_cost: self.switch_cost,
            cache: Arc::clone(&self.cache),
        }
    }
}

impl JapaneseSegmenter {
    pub fn new(english: &'static EnglishModel, japanese: &'static JapaneseModel) -> Self {
        Self {
            english,
            japanese,
            english_letter_log_p: -1.6,
            switch_cost: 4.0,
            cache: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The segments, or None when the keys compose cleanly (nothing to split) or no
    /// split with both Japanese and English beats leaving stray letters.
    pub fn segment(&self, keys: &str) -> Option<Vec<Segment>> {
        self.scored_segments(keys).map(|(segments, _)| segments)
    }

    /// Segments with their total log-probability (Japanese spans + English credit − switches).
    pub fn scored_segments(&self, keys: &str) -> Option<ScoredSegments> {
        if let Some(hit) = self.cache_get(keys) {
            return hit;
        }
        let result = self.search(keys);
        self.cache_set(keys, result.clone());
        result
    }

    fn cache_get(&self, keys: &str) -> Option<Option<ScoredSegments>> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .iter()
            .find(|(k, _)| k == keys)
            .map(|(_, result)| result.clone())
    }

    fn cache_set(&self, keys: &str, result: Option<ScoredSegments>) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(0, (keys.to_string(), result));
        cache.truncate(8);
    }

    /// `githubde` → github + で: a known English word and one case particle, the whole not
    /// itself English. Decided like `github에`, whatever the previous word was. Checked
    /// directly from the end, longest English word first.
    pub fn english_with_particle(&self, keys: &str) -> Option<(String, String)> {
        let chars: Vec<char> = keys.chars().collect();
        if chars.len() < 4
            || self.english.is_known_word(keys)
            || RomajiComposer::compose(keys, false).stray_letters == 0
        {
            return None;
        }
        let low = std::cmp::max(3, chars.len() - 4);
        for cut in (low..chars.len()).rev() {
            let front: String = chars[..cut].iter().collect();
            let tail = RomajiComposer::compose(&chars[cut..].iter().collect::<String>(), false);
            if !tail.is_clean() || !TRAILING_PARTICLES.contains(tail.text.as_str()) {
                continue;
            }
            if !front.chars().all(|c| c.is_alphabetic()) || !self.english.is_known_word(&front) {
                continue;
            }
            return Some((front, tail.text));
        }
        None
    }

    // MARK: - Search

    fn search(&self, keys: &str) -> Option<ScoredSegments> {
        if RomajiComposer::compose(keys, false).stray_letters == 0 {
            return None;
        }
        let chars: Vec<char> = keys.chars().collect();
        let n = chars.len();
        // Where known English words (3+ letters) are: end positions per start.
        let mut english_ends: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut cuts: BTreeSet<usize> = BTreeSet::new();
        cuts.insert(0);
        cuts.insert(n);
        for i in 0..n {
            if !chars[i].is_alphabetic() {
                continue;
            }
            let mut j = i;
            while j < n && j - i < MAX_ENGLISH_LENGTH && chars[j].is_alphabetic() {
                j += 1;
                if j - i >= 3
                    && self
                        .english
                        .is_known_word(&chars[i..j].iter().collect::<String>())
                {
                    english_ends[i].push(j);
                    cuts.insert(i);
                    cuts.insert(j);
                }
            }
        }
        if cuts.len() <= 2 && !english_ends[0].contains(&n) {
            return None;
        }
        let positions: Vec<usize> = cuts.iter().copied().collect();
        let index: HashMap<usize, usize> =
            positions.iter().enumerate().map(|(i, p)| (*p, i)).collect();
        // One Japanese span runs from the start or an English word's end to the end or an
        // English word's start (two spans side by side would only score worse).
        let english_starts: HashSet<usize> = english_ends
            .iter()
            .enumerate()
            .filter(|(_, ends)| !ends.is_empty())
            .map(|(i, _)| i)
            .collect();
        let english_end_set: HashSet<usize> = english_ends.iter().flatten().copied().collect();
        let japanese_from: Vec<bool> = positions
            .iter()
            .map(|p| *p == 0 || english_end_set.contains(p))
            .collect();
        let japanese_to: Vec<bool> = positions
            .iter()
            .map(|p| *p == n || english_starts.contains(p))
            .collect();

        // best[position][kana so far, capped at 3][used English]: the best path to there.
        let mut best: Vec<Vec<[Option<State>; 2]>> = (0..positions.len())
            .map(|_| (0..4).map(|_| [None, None]).collect())
            .collect();
        best[0][0][0] = Some(State {
            score: 0.0,
            back: None,
            segment: None,
        });
        for p in 0..positions.len() {
            for kana in 0..4usize {
                for used in 0..2usize {
                    let Some(state) = best[p][kana][used].clone() else {
                        continue;
                    };
                    let from = positions[p];
                    if from < n {
                        for end in english_ends[from].clone() {
                            let word: String = chars[from..end].iter().collect();
                            let score = self.english_letter_log_p * word.chars().count() as f64
                                - self.switch_cost;
                            let q = index[&end];
                            let total = state.score + score;
                            if best[q][kana][1].as_ref().is_none_or(|s| total > s.score) {
                                best[q][kana][1] = Some(State {
                                    score: total,
                                    back: Some((p, kana, used == 1)),
                                    segment: Some(Segment::English(word)),
                                });
                            }
                        }
                    }
                    if japanese_from[p] {
                        for q in (p + 1)..positions.len() {
                            if !japanese_to[q] {
                                continue;
                            }
                            let piece: String = chars[from..positions[q]].iter().collect();
                            // A letter no vowel can rescue (the g of "gl") fails every longer span too.
                            let reading = RomajiComposer::compose(&piece, true);
                            if reading.stray_letters > 0 {
                                break;
                            }
                            let composed = if reading.pending.is_empty() {
                                reading
                            } else {
                                RomajiComposer::compose(&piece, false)
                            };
                            // A one-kana span must be a particle (googleで), not a leftover (would + ん).
                            if !composed.is_clean()
                                || composed.text.is_empty()
                                || (composed.text.chars().count() < 2
                                    && !PARTICLES.contains(composed.text.as_str()))
                            {
                                continue;
                            }
                            // Each span scores as Japanese on its own (start/end markers included).
                            let kana_after = std::cmp::min(3, kana + composed.text.chars().count());
                            let score = self.japanese.log_probability(&composed.text, false);
                            let total = state.score + score;
                            if best[q][kana_after][used]
                                .as_ref()
                                .is_none_or(|s| total > s.score)
                            {
                                best[q][kana_after][used] = Some(State {
                                    score: total,
                                    back: Some((p, kana, used == 1)),
                                    segment: Some(Segment::Japanese {
                                        keys: piece,
                                        kana: composed.text,
                                    }),
                                });
                            }
                        }
                    }
                }
            }
        }

        // Needs real Japanese around the English: at least 3 kana (not mum + ばい), or exactly
        // an English word with a particle after it (github + で).
        let mut answer: Option<ScoredSegments> = None;
        let last = positions.len() - 1;
        for kana in 0..4usize {
            let Some(state) = best[last][kana][1].clone() else {
                continue;
            };
            let segments = Self::path((last, kana, true), &best);
            if kana < 3 && !self.ends_in_particle(&segments, keys) {
                continue;
            }
            if answer.as_ref().is_none_or(|(_, s)| state.score > *s) {
                answer = Some((Self::merged(segments), state.score));
            }
        }
        answer
    }

    fn path(end: (usize, usize, bool), best: &[Vec<[Option<State>; 2]>]) -> Vec<Segment> {
        let mut segments: Vec<Segment> = Vec::new();
        let mut at: Option<(usize, usize, bool)> = Some(end);
        while let Some(node) = at {
            let Some(state) = best[node.0][node.1][usize::from(node.2)].as_ref() else {
                break;
            };
            if let Some(segment) = &state.segment {
                segments.push(segment.clone());
            }
            at = state.back;
        }
        segments.reverse();
        segments
    }

    /// Exactly an English word plus one particle, where the whole isn't itself English.
    fn ends_in_particle(&self, segments: &[Segment], keys: &str) -> bool {
        if segments.len() != 2 {
            return false;
        }
        match (&segments[0], &segments[1]) {
            (Segment::English(_), Segment::Japanese { kana, .. }) => {
                TRAILING_PARTICLES.contains(kana.as_str()) && !self.english.is_known_word(keys)
            }
            _ => false,
        }
    }

    /// Adjacent Japanese spans join into one (きょうは + でけんさく stay separate only around English).
    fn merged(segments: Vec<Segment>) -> Vec<Segment> {
        let mut out: Vec<Segment> = Vec::new();
        for segment in segments {
            if let Segment::Japanese {
                keys: k2,
                kana: kana2,
            } = &segment
            {
                if let Some(Segment::Japanese {
                    keys: k1,
                    kana: kana1,
                }) = out.last().cloned()
                {
                    out.pop();
                    out.push(Segment::Japanese {
                        keys: format!("{k1}{k2}"),
                        kana: format!("{kana1}{kana2}"),
                    });
                    continue;
                }
            }
            out.push(segment);
        }
        out
    }

    // MARK: - Readings

    /// The final reading of romaji keys: kana, with English words kept when a segmenter finds them.
    pub fn reading_of(keys: &str, segmenter: Option<&JapaneseSegmenter>) -> String {
        if let Some(segments) = segmenter.and_then(|s| s.segment(keys)) {
            return Self::text(&segments);
        }
        RomajiComposer::compose(keys, false).text
    }

    /// Kana with the English words left in place: きょうはgoogleでけんさく.
    pub fn text(segments: &[Segment]) -> String {
        segments
            .iter()
            .map(|segment| match segment {
                Segment::Japanese { kana, .. } => kana.clone(),
                Segment::English(word) => word.clone(),
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segmenter() -> JapaneseSegmenter {
        JapaneseSegmenter::new(EnglishModel::shared(), JapaneseModel::shared())
    }

    #[test]
    fn splits_english_out_of_japanese() {
        for (keys, text) in [
            ("kyouhagoogledekensaku", "きょうはgoogleでけんさく"),
            ("meetingwoshimashou", "meetingをしましょう"),
            ("serverniloginshita", "serverにloginした"),
        ] {
            let segments = segmenter().segment(keys).expect(keys);
            assert_eq!(JapaneseSegmenter::text(&segments), text, "{keys}");
        }
    }

    #[test]
    fn leaves_other_input_alone() {
        for keys in ["konnichiha", "wouldn", "mumbai", "hello", "brooklyn"] {
            assert_eq!(segmenter().segment(keys), None, "{keys}");
        }
    }

    #[test]
    fn english_with_particle() {
        let s = segmenter();
        // guswo (현재) also finds gus+を here; the SplittingEngine is what keeps it Korean.
        for (keys, expected) in [
            ("githubde", Some(("github", "で"))),
            ("slackkara", Some(("slack", "から"))),
            ("jayne", None),
            ("photo", None),
        ] {
            assert_eq!(
                s.english_with_particle(keys),
                expected.map(|(w, p)| (w.to_string(), p.to_string())),
                "{keys}"
            );
        }
    }
}
