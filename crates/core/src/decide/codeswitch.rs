//! One score decides where English and Korean meet inside a token typed with no space:
//! `reportwha` → report + 좀, `nwp` → n + 제, `commitgotj` → commit + 해서.
//!
//! Every way to cut the keys into English and Korean stretches is scored on one scale (log
//! probability, so the totals compare), and the best cut wins. There is no rule per case: a
//! single letter, an acronym, a particle and a loanword are all just stretches with a score.
//! What is hard-coded is only what the keyboard makes impossible (a Korean stretch has to
//! compose to whole syllables); the weights are fitted by `lab -- --tune`.
//!
//! The first version composes from scratch on every call (tokens are at most `MAX_KEYS`
//! keys); the incremental table comes after the weights are tuned.

use std::cell::RefCell;

use crate::decide::engine::{Decision, DecisionContext, Language};
use crate::lang::english::EnglishModel;
use crate::lang::hangul::{is_consonant, ComposerState, DubeolsikLayout, HangulComposer, Unit};
use crate::lang::korean::{KoreanModel, PENDING_CONSONANT_LOGP, UNKNOWN_SYLLABLE_LOGP};
use crate::lang::particles::{is_ending, is_single_letter_particle};

/// Slots of the per-thread answer cache (16 bytes each, no heap).
const CACHE_SLOTS: usize = 512;

thread_local! {
    /// Word-list answers for the stretches of the token being typed: each keystroke asks about
    /// the same stretches again. Direct-mapped by a 64-bit hash of the stretch (English ones are
    /// ASCII and Korean ones Hangul, so they don't mix); an empty slot has hash 0.
    static KNOWN: RefCell<[(u64, Known); CACHE_SLOTS]> =
        const { RefCell::new([(0, Known { word: false, whole: false }); CACHE_SLOTS]) };
}

/// What the word lists say about a stretch, English or Korean.
#[derive(Clone, Copy)]
struct Known {
    /// English: a listed word. Korean: a known word, or a word with an ending.
    word: bool,
    /// Korean: a known word on its own.
    whole: bool,
}

/// FNV-1a, never 0 (0 marks an empty cache slot).
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    }) | 1
}

/// Counters that follow a number (년월일시분초 개명원번 살층대권).
const COUNTERS: &str = "년월일시분초개명원번살층대권";
/// Units written in Latin letters after a number.
const UNITS: [&str; 20] = [
    "kwh", "khz", "mhz", "ghz", "kg", "km", "cm", "mm", "ml", "mg", "gb", "mb", "kb", "tb", "hz",
    "ms", "px", "fps", "rpm", "kw",
];

const MIN_KEYS: usize = 2;
/// Shorter English words lead a Korean-reading token by chance (gus in gusrl).
const LEAD_WORD_KEYS: usize = 4;
const MAX_KEYS: usize = 32;
/// How sure a Mixed answer is, until the score gap sets it.
const CONFIDENCE: f64 = 0.98;

/// The weights of the score. All in log-probability units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Paid at each boundary between an English and a Korean stretch.
    pub switch_cost: f64,
    /// Per letter of an English stretch that is a listed word.
    pub dict_bonus: f64,
    /// Per letter of every English stretch: the prior of English over Korean for a key.
    pub en_bias: f64,
    /// For a stretch that is the whole token and a listed word (tango is not tan + 해).
    pub whole_bonus: f64,
    /// For a Korean stretch that is a particle or an ending on its own.
    pub ending_bonus: f64,
    /// The same for a one-syllable Korean stretch at the very start (fatso → 내 + fat…).
    pub lead_ko: f64,
    /// For a counter or unit stretch right after a digit.
    pub digit_bonus: f64,
    /// A label at the start (one letter, or 2-4 capitals: n제, A급, QR코드) as a whole. The
    /// word model would charge it for a word start and end that it doesn't have.
    pub label_logp: f64,
    /// For a Korean stretch behind English that is a listed word or one inflected (mysql + 이해를).
    pub word_bonus: f64,
    /// Per capital in a Korean stretch that no shifted jamo explains (the S of Slack).
    pub cap_penalty: f64,
    /// How far the best mixed cut has to beat the best single-language reading.
    pub margin: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            switch_cost: 5.0,
            dict_bonus: 1.0,
            en_bias: 0.5,
            whole_bonus: 6.0,
            ending_bonus: 4.0,
            lead_ko: -8.0,
            digit_bonus: 6.0,
            label_logp: -2.0,
            word_bonus: 0.0,
            cap_penalty: 6.0,
            margin: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    English,
    Korean,
}

impl Kind {
    const ALL: [Kind; 2] = [Kind::English, Kind::Korean];

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: Kind,
    pub keys: String,
    /// What the stretch shows: the keys for English, the composed Hangul for Korean.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Split {
    pub pieces: Vec<Piece>,
    pub score: f64,
    /// How far this cut is ahead of the best single-language reading (infinite if none exists).
    pub gap: f64,
}

impl Split {
    pub fn text(&self) -> String {
        self.pieces.iter().map(|p| p.text.as_str()).collect()
    }

    pub fn explanation(&self) -> String {
        let parts: Vec<&str> = self.pieces.iter().map(|p| p.text.as_str()).collect();
        format!("codeswitch {} ({:+.1})", parts.join("+"), self.gap)
    }
}

pub struct CodeSwitch {
    english: &'static EnglishModel,
    korean: KoreanModel,
    pub params: Params,
}

/// Best path into a position, with where it came from: (position, last kind, stretches).
#[derive(Clone, Copy)]
struct Cell {
    score: f64,
    from: (usize, usize, usize),
}

/// Per position, per kind of the last stretch, per stretches so far (0: one, 1: two or more).
type Table = Vec<[[Option<Cell>; 2]; 2]>;

impl CodeSwitch {
    pub fn new(english: &'static EnglishModel, params: Params) -> Self {
        Self {
            english,
            korean: KoreanModel::default(),
            params,
        }
    }

    pub fn margin(&self) -> f64 {
        self.params.margin
    }

    /// A Mixed decision when the best cut has both languages and beats every single-language
    /// reading by `margin`; None leaves the token to the engine behind.
    pub fn decide(&self, keys: &str, context: &DecisionContext) -> Option<Decision> {
        if context.after_digit {
            if let Some(decision) = Self::digit_word(keys) {
                return Some(decision);
            }
        }
        let split = self.split(keys, context)?;
        Some(Decision {
            language: Language::Mixed,
            confidence: CONFIDENCE,
            output: split.text(),
            explanation: split.explanation(),
            runner_up: None,
        })
    }

    /// Right after a digit a counter is Korean (2024년, 3개를) and a unit English (1.5kg): spelled
    /// alone they read the other way (sus is a word, kg makes no syllable). With an ending
    /// behind a unit the search cuts it (kg이다).
    fn digit_word(keys: &str) -> Option<Decision> {
        let text = HangulComposer::compose(keys)
            .filter(|c| c.units.iter().all(|u| matches!(u, Unit::Syllable(_))))?
            .text();
        let (language, output, why) = if is_counter_word(&text) {
            (Language::Korean, text, "counter after a digit")
        } else if UNITS.contains(&keys.to_ascii_lowercase().as_str()) {
            (Language::English, keys.to_string(), "unit after a digit")
        } else {
            return None;
        };
        Some(Decision {
            language,
            confidence: CONFIDENCE,
            output,
            explanation: why.to_string(),
            runner_up: None,
        })
    }

    /// The best cut with both languages when it beats the single-language readings by `margin`.
    pub fn split(&self, keys: &str, context: &DecisionContext) -> Option<Split> {
        self.analyze(keys, context)
            .filter(|s| s.gap >= self.params.margin)
    }

    /// The best cut with both languages and how far ahead of the single-language readings it
    /// is, whatever the margin (`gap` may be negative). None when no such cut exists.
    pub fn analyze(&self, keys: &str, context: &DecisionContext) -> Option<Split> {
        let n = keys.len();
        if !(MIN_KEYS..=MAX_KEYS).contains(&n) || !keys.bytes().all(|b| b.is_ascii_alphabetic()) {
            return None;
        }
        // A whole token that is already a listed English word or a clean Korean word is not
        // worth a search (it would only win by a bonus it already has). After a digit it may
        // still be a counter or a unit.
        if !context.after_digit
            && (self.is_english_word(keys)
                || (self.reads_as_korean(keys, context.partial) && !self.starts_with_word(keys)))
        {
            return None;
        }
        let bytes = keys.as_bytes();
        let width = n + 1;
        // The English stretch from the start to `j` is a label (n제, A급, QR코드).
        let label: Vec<bool> = (0..=n).map(|j| j > 0 && is_label(&keys[..j])).collect();

        // Korean stretches first: only they can be missing, and the English side is worked out
        // only where one can touch them (stretches alternate).
        let mut korean: Vec<Option<f64>> = vec![None; width * width];
        let mut starts = vec![false; width];
        let mut ends = vec![false; width];
        let mut word = String::new();
        for i in 0..n {
            // A syllable starts on a consonant key; a stretch led by a vowel is all strays.
            if !DubeolsikLayout::starts_syllable(bytes[i] as char) {
                continue;
            }
            let mut state = ComposerState::default();
            let mut dead = false;
            for j in i + 1..=n {
                state.push(DubeolsikLayout::jamo(bytes[j - 1] as char)?);
                if let Some(score) = self.korean_score(
                    &keys[i..j],
                    &state,
                    (i == 0, j == n, label[i], i == 1),
                    &mut word,
                    &mut dead,
                    context,
                ) {
                    korean[i * width + j] = Some(score);
                    starts[i] = true;
                    ends[j] = true;
                } else if dead {
                    break;
                }
            }
        }
        if !starts.iter().any(|s| *s) {
            return None;
        }
        let mut english: Vec<Option<f64>> = vec![None; width * width];
        for i in 0..n {
            if i > 0 && !ends[i] {
                continue;
            }
            for j in i + 1..=n {
                // A lone letter is English only as a label in front (n제), never mid-token.
                if (j - i == 1 && i > 0) || !(j == n || starts[j]) {
                    continue;
                }
                english[i * width + j] =
                    Some(self.english_score(&keys[i..j], i == 0, j == n, context));
            }
        }

        let table = self.search(n, &label, |kind, i, j| match kind {
            Kind::English => english[i * width + j],
            Kind::Korean => korean[i * width + j],
        });
        let mixed = (0..2)
            .filter_map(|k| table[n][k][1].map(|c| (c.score, k)))
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        let pure = [table[n][0][0], table[n][1][0]]
            .into_iter()
            .flatten()
            .map(|c| c.score)
            .max_by(f64::total_cmp);
        let gap = pure.map_or(f64::INFINITY, |p| mixed.0 - p);

        let mut pieces = Vec::new();
        let (mut pos, mut kind, mut count) = (n, mixed.1, 1);
        while pos > 0 {
            let cell = table[pos][kind][count]?;
            let start = cell.from.0;
            let stretch = &keys[start..pos];
            pieces.push(if kind == Kind::English.index() {
                Piece {
                    kind: Kind::English,
                    keys: stretch.to_string(),
                    text: stretch.to_string(),
                }
            } else {
                Piece {
                    kind: Kind::Korean,
                    keys: stretch.to_string(),
                    text: HangulComposer::compose(stretch)?.text(),
                }
            });
            (pos, kind, count) = (start, cell.from.1, cell.from.2);
        }
        pieces.reverse();
        Some(Split {
            pieces,
            score: mixed.0,
            gap,
        })
    }

    /// The word-list answer for `text`, asked of the models only when the cache lacks it.
    fn known(&self, text: &str, ask: impl FnOnce(&str) -> Known) -> Known {
        let hash = fingerprint(text);
        let slot = hash as usize % CACHE_SLOTS;
        KNOWN.with(|cache| {
            let hit = cache.borrow()[slot];
            if hit.0 == hash {
                return hit.1;
            }
            let known = ask(text);
            cache.borrow_mut()[slot] = (hash, known);
            known
        })
    }

    /// Whether a listed English word of at least `LEAD_WORD_KEYS` letters leads the token
    /// (slack in slackdptj, which also reads as Korean): the search may still prefer it.
    fn starts_with_word(&self, keys: &str) -> bool {
        (LEAD_WORD_KEYS..keys.len()).any(|end| self.is_english_word(&keys[..end]))
    }

    /// Whether the whole token reads as Korean on its own: it composes to syllables people
    /// actually use (the next syllable's first consonant is fine while it is still being
    /// typed). Such a token needs no search.
    fn reads_as_korean(&self, keys: &str, partial: bool) -> bool {
        // A capital that no shifted jamo explains was typed as English on purpose.
        if keys
            .chars()
            .any(|c| c.is_uppercase() && !DubeolsikLayout::is_korean_shift_key(c))
        {
            return false;
        }
        let Some(composed) = HangulComposer::compose(keys) else {
            return false;
        };
        let last = composed.units.len().saturating_sub(1);
        let mut syllables = 0usize;
        for (index, unit) in composed.units.iter().enumerate() {
            match unit {
                Unit::Syllable(s) => {
                    if self.korean.syllable_log_probability(*s) <= UNKNOWN_SYLLABLE_LOGP {
                        return false;
                    }
                    syllables += 1;
                }
                // The next syllable's first consonant, while the token is still being typed.
                Unit::Jamo(j) if partial && index == last && is_consonant(*j) => {}
                Unit::Jamo(_) => return false,
            }
        }
        syllables > 0
    }

    fn is_english_word(&self, text: &str) -> bool {
        self.known(text, |_| Known {
            word: self.english.is_known_word(text),
            whole: false,
        })
        .word
    }

    fn korean_known(&self, word: &str) -> Known {
        // The wider vocabulary counts too: loanwords (메신저, 서버) are no everyday word alone.
        self.known(word, |w| {
            let whole = self.korean.is_vocabulary_word(w);
            Known {
                word: whole || self.korean.is_inflected_vocabulary_word(w),
                whole,
            }
        })
    }

    /// The best score into every position. English and Korean stretches alternate (two of the
    /// same kind side by side are one longer stretch, and would only collect its bonus twice).
    /// `label[j]` says the English stretch from the start to `j` is a label.
    /// `emit(kind, from, to)` is a stretch's score, None when it can't exist.
    fn search(
        &self,
        n: usize,
        label: &[bool],
        emit: impl Fn(Kind, usize, usize) -> Option<f64>,
    ) -> Table {
        let mut table: Table = vec![[[None; 2]; 2]; n + 1];
        let relax = |table: &mut Table, j: usize, kind: Kind, count: usize, cell: Cell| {
            let slot = &mut table[j][kind.index()][count];
            if slot.is_none_or(|old| cell.score > old.score) {
                *slot = Some(cell);
            }
        };
        for j in 1..=n {
            for i in 0..j {
                for kind in Kind::ALL {
                    let Some(score) = emit(kind, i, j) else {
                        continue;
                    };
                    if i == 0 {
                        let cell = Cell {
                            score,
                            from: (0, 0, 0),
                        };
                        relax(&mut table, j, kind, 0, cell);
                        continue;
                    }
                    let last = 1 - kind.index();
                    for count in 0..2 {
                        let Some(prev) = table[i][last][count] else {
                            continue;
                        };
                        // A label at the start names the one Korean word that ends the token.
                        if kind == Kind::Korean && count == 0 && label[i] && j != n {
                            continue;
                        }
                        let cell = Cell {
                            score: prev.score + score - self.params.switch_cost,
                            from: (i, last, count),
                        };
                        relax(&mut table, j, kind, 1, cell);
                    }
                }
            }
        }
        table
    }

    /// An English stretch: letter-trigram log probability, plus a bonus when it is a word.
    fn english_score(
        &self,
        seg: &str,
        at_start: bool,
        at_end: bool,
        context: &DecisionContext,
    ) -> f64 {
        if at_start && is_label(seg) {
            return self.params.label_logp;
        }
        let len = seg.len() as f64;
        let mut score = if context.partial && at_end {
            self.english.prefix_log_probability(seg)
        } else {
            self.english.total_log_probability(seg)
        };
        score += self.params.en_bias * len;
        if self.is_english_word(seg) {
            score += self.params.dict_bonus * len;
            if at_start && at_end {
                score += self.params.whole_bonus;
            }
        }
        if context.after_digit && at_start && UNITS.contains(&seg.to_ascii_lowercase().as_str()) {
            score += self.params.digit_bonus;
        }
        score
    }

    /// A Korean stretch: syllable log probabilities, plus a bonus when it is a word, an ending
    /// or a counter. None when the keys leave a stray jamo (they can't be Korean) or make no
    /// whole syllable.
    fn korean_score(
        &self,
        keys: &str,
        composed: &ComposerState,
        (at_start, at_end, after_label, after_letter): (bool, bool, bool, bool),
        word: &mut String,
        dead: &mut bool,
        context: &DecisionContext,
    ) -> Option<f64> {
        let mut score = 0.0;
        let mut syllables = 0usize;
        let mut feasible = true;
        let mut pending = false;
        word.clear();
        composed.for_each_unit(|unit| {
            // A stray before a later unit stays a stray however long the stretch gets.
            *dead |= !feasible;
            // Only the last unit may be the next syllable's first consonant (token still typed).
            feasible &= !pending;
            match unit {
                Unit::Syllable(s) => {
                    score += self.korean.syllable_log_probability(s);
                    syllables += 1;
                    word.push(s);
                }
                Unit::Jamo(j) if context.partial && at_end && is_consonant(j) => {
                    score += PENDING_CONSONANT_LOGP;
                    pending = true;
                }
                Unit::Jamo(_) => feasible = false,
            }
        });
        if !feasible || syllables == 0 {
            return None;
        }
        let capitals = keys
            .chars()
            .filter(|c| c.is_uppercase() && !DubeolsikLayout::is_korean_shift_key(*c))
            .count();
        score -= self.params.cap_penalty * capitals as f64;
        // An ending needs a word before it to attach to.
        // After a single letter only a case particle does (x에, X를), not 해 or 야.
        let ending =
            !at_start && is_ending(word) && (!after_letter || is_single_letter_particle(word));
        // The noun a letter names is no ending of another kind (edi is no e + 야).
        if after_letter && is_ending(word) && !is_single_letter_particle(word) {
            return None;
        }
        // English follows: it needs two syllables of evidence (below), so don't look up fewer.
        if !at_end && syllables < 2 {
            return None;
        }
        let counter = context.after_digit && at_start && is_counter_word(word);
        let known = if counter || ending {
            Known {
                word: true,
                whole: false,
            }
        } else {
            self.korean_known(word)
        };
        // A stretch with no evidence (no word, no ending) must be the whole token or a tail of
        // two syllables or more; one with English after it has to be two syllables or more and an
        // ending or a real word (come야etta is no come + 야 + etta).
        if !(at_start && at_end) {
            let strong = syllables >= 2 && (ending || counter || known.word);
            // The noun a label names may be one syllable that no list has (n제, x축).
            if (!known.word && syllables < 2 && !after_label) || (!at_end && !strong) {
                return None;
            }
        }
        if known.word && !at_start && !ending {
            score += self.params.word_bonus;
        }
        if counter {
            score += self.params.digit_bonus;
        } else if ending {
            score += self.params.ending_bonus;
        } else if known.whole && at_start && at_end {
            score += self.params.whole_bonus;
        }
        // One syllable led by nothing but the start: weak evidence next to English (fatso).
        if syllables == 1 && at_start && !counter && !ending {
            score += self.params.lead_ko;
        }
        Some(score)
    }
}

/// One letter, or 2-4 capitals: what is typed in front of a word to name it (n제, A급, QR코드).
fn is_label(seg: &str) -> bool {
    seg.len() == 1 || ((2..=4).contains(&seg.len()) && seg.chars().all(|c| c.is_ascii_uppercase()))
}

/// A counter (년월일…) alone or with an ending: 2024년, 3개를.
fn is_counter_word(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let rest = chars.as_str();
    COUNTERS.contains(first) && (rest.is_empty() || is_ending(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(keys: &str, context: &DecisionContext) -> Option<String> {
        let cs = CodeSwitch::new(EnglishModel::shared(), Params::default());
        cs.split(keys, context).map(|s| s.text())
    }

    fn split(keys: &str) -> Option<String> {
        cut(keys, &DecisionContext::default())
    }

    #[test]
    fn cuts_english_from_korean_by_score() {
        for (keys, text) in [
            ("githubdp", "github에"),
            ("reportwha", "report좀"),
            ("kakaotalkaptlswj", "kakaotalk메신저"),
            ("KakaoTalkaptlswj", "KakaoTalk메신저"),
            ("nwp", "n제"),
            ("xcnr", "x축"),
            ("xdp", "x에"),
            ("QRzhem", "QR코드"),
            ("commitgotj", "commit해서"),
            ("mongodbfmf", "mongodb를"),
            ("slackdptj", "slack에서"),
        ] {
            assert_eq!(split(keys).as_deref(), Some(text), "{keys}");
        }
    }

    /// Every one of these was once cut by a rule too eager for English or Korean: a Korean
    /// stretch with no evidence, a lone letter that is no label, a syllable between two English
    /// stretches.
    #[test]
    fn leaves_single_language_tokens_alone() {
        for keys in [
            "weekend",
            "keyword",
            "something",
            "understand",
            "network",
            "hello",
            "dkssudgktpdy",
            "cmdr",
            "edi",
            "edit",
            "bro",
            "ceo",
            "Edmundo",
            "halal",
            "exceptionalism",
            "comedietta",
            "godlily",
            "woaldjqtdmfrjrkxdk",
        ] {
            assert_eq!(split(keys), None, "{keys}");
        }
    }

    /// A word still being typed is no word end yet: no prefix of it is cut.
    #[test]
    fn typing_an_english_word_cuts_no_prefix() {
        let typing = DecisionContext {
            partial: true,
            ..Default::default()
        };
        for word in ["predisposable", "understanding", "kubernetes"] {
            for end in 2..=word.len() {
                assert_eq!(cut(&word[..end], &typing), None, "{}", &word[..end]);
            }
        }
    }
}
