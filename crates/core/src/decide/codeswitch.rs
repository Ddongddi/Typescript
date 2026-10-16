//! English and Korean typed as one token with no space (`commitgotj` → commit해서, a whole
//! story typed in one go) are read as a path through a lattice of words.
//!
//! Every way to read a stretch of keys as one word is an edge: a listed English word, an English
//! run that is none, a listed Korean word, a particle or ending, a Korean syllable that is in no
//! word. An edge costs its log probability; a join costs what the step from one kind of edge
//! to the next costs (a particle after English is cheap, a stray English run between Korean words
//! is not). The best path wins, and it is Mixed when it beats every single-language path by
//! `margin`. There is no rule per case: the evidence a Korean stretch needs, a compound like
//! fortyminutes, a lone particle between English words all come out of the same scores.
//!
//! What is hard-coded is only what the keyboard makes impossible (a Korean edge has to compose to
//! whole syllables); the weights are fitted by `lab -- --tune`.

use std::cell::RefCell;

use crate::decide::engine::{Decision, DecisionContext, Language};
use crate::lang::english::{EnglishModel, Walk};
use crate::lang::hangul::{is_consonant, ComposerState, DubeolsikLayout, HangulComposer, Unit};
use crate::lang::korean::{KoreanModel, WordLines, PENDING_CONSONANT_LOGP, UNKNOWN_SYLLABLE_LOGP};
use crate::lang::particles::{is_ending, is_single_letter_particle};

const MIN_KEYS: usize = 2;
/// A token is searched whole at any length: the cost grows with it, not with a window.
const MAX_KEYS: usize = 4096;
/// Longest English edge (a word, or a run that is none); a longer stretch is several edges.
const EN_MAX: usize = 24;
const KO_MAX_SYLLABLES: usize = 8;
/// No edge is longer than this many keys (8 syllables of 5 keys, and a pending consonant), so a
/// key added at the end changes nothing that starts earlier.
const LOOKBACK: usize = KO_MAX_SYLLABLES * 5 + 8;
/// Shorter English words lead a Korean-reading token by chance (gus in gusrl).
const LEAD_WORD_KEYS: usize = 4;
/// A token that comes in longer than this at once is searched in pieces of this many keys.
const CHUNK: usize = 128;
/// A lone syllable counts as a Korean word when it is said more than about once in a thousand
/// words (좀 and 해 are, 맥 and 리 are not).
const LONE_KNOWN_LOGP: f64 = -7.0;
/// How sure a Mixed answer is.
const CONFIDENCE: f64 = 0.98;

/// Counters that follow a number (년월일시분초 개명원번 살층대권).
const COUNTERS: &str = "년월일시분초개명원번살층대권";
/// Units written in Latin letters after a number.
pub(crate) const UNITS: [&str; 21] = [
    "kwh", "khz", "mhz", "ghz", "kg", "km", "cm", "mm", "ml", "mg", "gb", "mb", "kb", "tb", "hz",
    "ms", "px", "fps", "rpm", "kw", "x",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    English,
    Korean,
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

/// The weights of the score. All in log-probability units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Paid at each boundary between an English and a Korean edge.
    pub switch_cost: f64,
    /// Paid by an English edge that is no listed word (on top of its letter probabilities).
    pub en_oov: f64,
    /// Paid per Korean syllable that is in no word (on top of its probability).
    pub ko_oov: f64,
    /// A particle or ending as an edge.
    pub ending_logp: f64,
    /// Between two English edges, and between two Korean ones (words run together).
    pub en_join: f64,
    pub ko_join: f64,
    /// Extra for a switch that lands on an edge with no word behind it: a Korean syllable
    /// after English (nod + 던), an English run after Korean (한 + vuf).
    pub ko_enter: f64,
    pub en_enter: f64,
    /// A one-syllable Korean edge at the very start: weak evidence next to English (fatso).
    pub lead_ko: f64,
    /// For a counter or unit right after a digit.
    pub digit_bonus: f64,
    /// A label at the start (one letter, or 2-4 capitals: n제, A급, QR코드) as a whole.
    pub label_logp: f64,
    /// A label names the noun that follows: that switch costs this much less.
    pub label_noun: f64,
    /// Per capital in a Korean edge that no shifted jamo explains (the S of Slack).
    pub cap_penalty: f64,
    /// How far the best mixed path has to beat the best single-language one.
    pub margin: f64,
    /// Extra when no Korean word, case particle or label noun is in the mixed path: Korean
    /// that is only strays and endings (the 맥 of aortosclerosis) is what any key string makes.
    pub weak_margin: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            switch_cost: 5.0,
            en_oov: 5.0,
            ko_oov: 1.5,
            ending_logp: 0.0,
            en_join: 3.0,
            ko_join: 3.0,
            ko_enter: 2.0,
            en_enter: 6.0,
            lead_ko: -4.0,
            digit_bonus: 6.0,
            label_logp: -10.0,
            label_noun: 3.5,
            cap_penalty: 10.0,
            margin: 0.0,
            weak_margin: 8.0,
        }
    }
}

#[cfg(feature = "tuning")]
impl Params {
    /// `TS_PARAMS="switch_cost=5,en_oov=4"` overrides the named weights (the lab's searches).
    fn with_env(mut self) -> Self {
        let Ok(spec) = std::env::var("TS_PARAMS") else {
            return self;
        };
        for (name, value) in spec.split(',').filter_map(|kv| kv.split_once('=')) {
            let Ok(v) = value.parse::<f64>() else {
                continue;
            };
            match name {
                "switch_cost" => self.switch_cost = v,
                "en_oov" => self.en_oov = v,
                "ko_oov" => self.ko_oov = v,
                "ending_logp" => self.ending_logp = v,
                "en_join" => self.en_join = v,
                "ko_join" => self.ko_join = v,
                "ko_enter" => self.ko_enter = v,
                "en_enter" => self.en_enter = v,
                "lead_ko" => self.lead_ko = v,
                "digit_bonus" => self.digit_bonus = v,
                "label_logp" => self.label_logp = v,
                "label_noun" => self.label_noun = v,
                "cap_penalty" => self.cap_penalty = v,
                "margin" => self.margin = v,
                "weak_margin" => self.weak_margin = v,
                _ => {}
            }
        }
        self
    }
}

/// What kind of word an edge is; the joins are priced between these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    EnWord,
    EnOov,
    EnLabel,
    KoWord,
    KoOov,
    /// A particle or ending that is no case particle (해, 한, 이야).
    KoEnding,
    /// A case particle (에, 를, 에서): also fine after a label (x에).
    KoCase,
}

const CLASSES: [Class; 7] = [
    Class::EnWord,
    Class::EnOov,
    Class::EnLabel,
    Class::KoWord,
    Class::KoOov,
    Class::KoEnding,
    Class::KoCase,
];

impl Class {
    fn kind(self) -> Kind {
        match self {
            Class::EnWord | Class::EnOov | Class::EnLabel => Kind::English,
            _ => Kind::Korean,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy)]
struct Edge {
    /// Where the edge ends (a key position).
    to: u32,
    class: Class,
    score: f64,
    /// A lone consonant: the next syllable's first key, while the token is still typed.
    pending: bool,
    /// One syllable that is also a particle or ending (야, 해): no noun for a label.
    ending_like: bool,
    /// One syllable that is a word of its own, said often (좀, 해): evidence of Korean.
    known: bool,
}

impl Edge {
    fn end(&self) -> usize {
        self.to as usize
    }

    fn new(to: usize, class: Class, score: f64) -> Self {
        Self {
            to: to as u32,
            class,
            score,
            pending: false,
            ending_like: false,
            known: false,
        }
    }
}

/// States of a path: bit 0 is whether it has switched language, bit 1 whether a Korean word,
/// case particle or label noun is on it.
const STATES: usize = 4;
/// The cells of a position: one per class of the last edge and state of the path.
const CELLS: usize = 7 * STATES;
const NO_SCORE: f64 = f64::NEG_INFINITY;
/// The class of the cell a path starts from.
const START: usize = 7;
/// Where the best path into a cell came from: how many keys back (6 bits), the class (3 bits,
/// `START` when the path starts there) and the state (2 bits) of the cell it came from. 0: no
/// path.
type Back = u16;

/// The best path into every position. What the next search needs (scores) is kept for the last
/// positions only; where each path came from is kept for all, to read the path back.
struct Table {
    back: Vec<[Back; CELLS]>,
    /// Scores of the positions from `base` on.
    scores: Vec<[f64; CELLS]>,
    base: usize,
}

fn cell(class: usize, state: usize) -> usize {
    class * STATES + state
}

impl Table {
    fn new() -> Self {
        Self {
            back: Vec::new(),
            scores: Vec::new(),
            base: 0,
        }
    }

    /// Keeps the first `len` positions.
    fn truncate(&mut self, len: usize) {
        self.back.truncate(len);
        self.scores.truncate(len.saturating_sub(self.base));
    }

    fn resize(&mut self, len: usize) {
        self.back.resize(len, [0; CELLS]);
        self.scores.resize(len - self.base, [NO_SCORE; CELLS]);
    }

    /// Forgets the scores before `floor`.
    fn trim(&mut self, floor: usize) {
        self.scores.drain(..floor - self.base);
        self.base = floor;
    }

    fn score(&self, pos: usize, class: usize, state: usize) -> Option<f64> {
        let score = self.scores[pos - self.base][cell(class, state)];
        (score > NO_SCORE).then_some(score)
    }

    /// Records the path into a cell when it beats the one there: `total` its score, coming
    /// from the cell `(position, class, state)`.
    fn offer(
        &mut self,
        pos: usize,
        (class, state): (usize, usize),
        total: f64,
        (from, prev_class, prev_state): (usize, usize, usize),
    ) {
        let at = cell(class, state);
        let slot = &mut self.scores[pos - self.base][at];
        if total > *slot {
            debug_assert!(pos - from < 64, "an edge is at most LOOKBACK keys");
            *slot = total;
            self.back[pos][at] =
                (pos - from) as Back | (prev_class as Back) << 6 | (prev_state as Back) << 9;
        }
    }

    /// Where the path into a cell came from: (position, class, state).
    fn from(&self, pos: usize, class: usize, state: usize) -> Option<(usize, usize, usize)> {
        let back = self.back[pos][cell(class, state)];
        (back != 0).then(|| {
            let back = usize::from(back);
            (pos - (back & 63), back >> 6 & 7, back >> 9 & 3)
        })
    }
}

/// The lattice of the token searched last, to extend by the next key.
struct Cache {
    params: Params,
    after_digit: bool,
    keys: String,
    edges: Vec<Vec<Edge>>,
    table: Table,
    walks: Walks,
    /// The edges of one start that the search goes over (reused).
    fresh: Vec<u32>,
    /// What ends before this has been dropped (edges, scores): a search can only go on from
    /// a token that shares more than `LOOKBACK` keys with it past this.
    floor: usize,
}

/// The English word list and letter model walked along the letters from each start key: the
/// first key's walk (the whole token is asked about it) and one per start of the last
/// `EN_MAX` keys, in slots by start.
struct Walks {
    head: Walk,
    ring: Vec<(usize, Walk)>,
    /// What the keys from each of the last `LOOKBACK` start keys compose to, by start.
    korean: Vec<KoWalk>,
}

/// The Hangul that the keys from one start key compose to, kept between keystrokes: a new key
/// is pushed into the composition and judged, where composing the stretch again would start
/// from its first key.
struct KoWalk {
    start: usize,
    /// The keys pushed so far.
    keys: Vec<u8>,
    state: ComposerState,
    /// Whether a stray jamo sat before a later unit in a stretch up to the key before the
    /// last (no longer stretch can be Korean), and whether the stretches stopped there.
    dead: bool,
    over: bool,
    /// Capitals that no shifted jamo explains.
    capitals: usize,
    /// What judging the stretch of that many keys said: (keys, dead after it, stopped). The next
    /// key pushed needs it, and it is the same however the stretch was judged (as the end of
    /// the token or not: only whether a lone first consonant is fine differs, which leaves
    /// both flags as they are).
    judged: Option<(usize, bool, bool)>,
    /// The composed Hangul of the last stretch judged.
    word: String,
    /// The words that start like the first `closed` syllables of `word`.
    closed: usize,
    lines: WordLines,
}

/// A stretch judged: no longer one can be Korean, it can't be, or it is.
enum Verdict {
    Stop,
    Skip,
    Fit {
        syllables: usize,
        pending: bool,
        /// Sum of the probabilities of its syllables.
        oov: f64,
    },
}

impl KoWalk {
    fn new() -> Self {
        Self {
            start: 0,
            keys: Vec::new(),
            state: ComposerState::default(),
            dead: false,
            over: false,
            capitals: 0,
            judged: None,
            word: String::new(),
            closed: 0,
            lines: KoreanModel::word_lines(),
        }
    }

    fn clear(&mut self) {
        self.keys.clear();
        self.state.clear();
        self.dead = false;
        self.over = false;
        self.capitals = 0;
        self.judged = None;
        self.closed = 0;
        self.lines = KoreanModel::word_lines();
    }

    /// Judges the stretch of the keys pushed. `at_end`: the last key of the token, which may
    /// leave the next syllable's first consonant alone while it is still typed. Also says
    /// whether the walk is dead after it.
    fn judge(&mut self, korean: &KoreanModel, at_end: bool, partial: bool) -> (Verdict, bool) {
        if self.over {
            return (Verdict::Stop, self.dead);
        }
        let (mut dead, mut syllables) = (self.dead, 0usize);
        let (mut feasible, mut pending, mut oov) = (true, false, 0.0);
        let word = &mut self.word;
        word.clear();
        self.state.for_each_unit(|unit| {
            // A stray before a later unit stays a stray however long the stretch gets.
            dead |= !feasible;
            // Only the last unit may be the next syllable's first consonant.
            feasible &= !pending;
            match unit {
                Unit::Syllable(s) => {
                    oov += korean.plausible_log_probability(s);
                    syllables += 1;
                    word.push(s);
                }
                Unit::Jamo(jamo) if partial && at_end && is_consonant(jamo) => pending = true,
                Unit::Jamo(_) => feasible = false,
            }
        });
        let verdict = if !feasible {
            if dead {
                Verdict::Stop
            } else {
                Verdict::Skip
            }
        } else if syllables > KO_MAX_SYLLABLES {
            Verdict::Stop
        } else {
            Verdict::Fit {
                syllables,
                pending,
                oov,
            }
        };
        (verdict, dead)
    }

    /// Pushes the next key. The stretch up to the key before is judged now that it is no
    /// longer the last (that is when a stray before a later unit shows).
    fn push(&mut self, korean: &KoreanModel, key: u8) -> Option<()> {
        let jamo = DubeolsikLayout::jamo(key as char)?;
        if !self.keys.is_empty() {
            let (dead, stop) = match self.judged.take() {
                Some((len, dead, stop)) if len == self.keys.len() => (dead, stop),
                _ => {
                    let (verdict, dead) = self.judge(korean, false, false);
                    (dead, matches!(verdict, Verdict::Stop))
                }
            };
            self.dead = dead;
            self.over |= stop;
        }
        self.state.push(jamo);
        self.keys.push(key);
        let c = key as char;
        self.capitals += usize::from(c.is_uppercase() && !DubeolsikLayout::is_korean_shift_key(c));
        Some(())
    }

    /// Brings the walk to exactly `len` keys of `keys` (from the start key on), keeping what
    /// is already walked when it is the same keys.
    fn sync(&mut self, korean: &KoreanModel, keys: &[u8], len: usize) -> Option<()> {
        let same = self
            .keys
            .iter()
            .zip(keys)
            .take_while(|(a, b)| a == b)
            .count();
        if same < self.keys.len() || self.keys.len() > len {
            self.clear();
        }
        while self.keys.len() < len && !self.over {
            self.push(korean, keys[self.keys.len()])?;
        }
        Some(())
    }

    /// The word lists narrowed to the words that start like the closed syllables of `word`.
    fn narrow_lines(&mut self, korean: &KoreanModel) {
        let closed = self.state.closed_len();
        while self.closed < closed {
            self.closed += 1;
            let end = self
                .word
                .char_indices()
                .nth(self.closed)
                .map_or(self.word.len(), |(at, _)| at);
            self.lines = korean.narrow(self.lines, &self.word[..end]);
        }
    }
}

impl Walks {
    fn new() -> Self {
        Self {
            head: Walk::default(),
            ring: Vec::new(),
            korean: Vec::new(),
        }
    }

    /// The walk of the Hangul from start key `i`.
    fn korean_of(&mut self, i: usize) -> &mut KoWalk {
        let slot = i % LOOKBACK;
        if self.korean.len() <= slot {
            self.korean.resize_with(slot + 1, KoWalk::new);
        }
        let slot = &mut self.korean[slot];
        if slot.start != i {
            slot.clear();
            slot.start = i;
        }
        slot
    }

    /// The walk of the stretches that start at key `i` (empty if it was another start's).
    fn of(&mut self, i: usize) -> &mut Walk {
        if i == 0 {
            return &mut self.head;
        }
        let slot = i % EN_MAX;
        if self.ring.len() <= slot {
            self.ring.resize_with(slot + 1, || (0, Walk::default()));
        }
        let slot = &mut self.ring[slot];
        if slot.0 != i {
            *slot = (i, Walk::default());
        }
        &mut slot.1
    }
}

/// Hangul syllables, and the 64-bit words that hold a bit for each.
const SYLLABLES: usize = 11172;
const SYLLABLE_WORDS: usize = SYLLABLES.div_ceil(64);

thread_local! {
    /// The composer `reads_as_korean` keeps its allocation in.
    static COMPOSER: RefCell<ComposerState> = RefCell::new(ComposerState::default());
    /// Per syllable whether it was looked up (the first half), then whether it is a frequent
    /// word (the second).
    static FREQUENT: RefCell<[u64; 2 * SYLLABLE_WORDS]> =
        const { RefCell::new([0; 2 * SYLLABLE_WORDS]) };
    static LAST: RefCell<Option<Cache>> = const { RefCell::new(None) };
}

pub struct CodeSwitch {
    english: &'static EnglishModel,
    korean: KoreanModel,
    pub params: Params,
}

impl CodeSwitch {
    pub fn new(english: &'static EnglishModel, params: Params) -> Self {
        #[cfg(feature = "tuning")]
        let params = params.with_env();
        Self {
            english,
            korean: KoreanModel::default(),
            params,
        }
    }

    pub fn margin(&self) -> f64 {
        self.params.margin
    }

    /// A Mixed decision when the best path has both languages and beats every single-language
    /// reading by `margin`; None leaves the token to the engine behind.
    pub fn decide(&self, keys: &str, context: &DecisionContext) -> Option<Decision> {
        if context.after_digit {
            if let Some(decision) = Self::digit_word(keys).or_else(|| {
                // A digit behind the token too (3x…p95): the search reads all of it.
                (!context.before_digit)
                    .then(|| self.unit_lead(keys, context.partial))
                    .flatten()
            }) {
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
    /// alone they read the other way (sus is a word, kg makes no syllable).
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

    /// Right after a digit, a unit followed by Korean is that unit (10cm는, 2gb가): cm and gb
    /// also spell 츠 and 휴, and that reading would otherwise win the whole token.
    fn unit_lead(&self, keys: &str, partial: bool) -> Option<Decision> {
        let lower = keys.to_ascii_lowercase();
        let (unit, rest) = UNITS
            .iter()
            .filter(|u| lower.starts_with(**u) && keys.len() > u.len())
            .max_by_key(|u| u.len())
            .map(|u| keys.split_at(u.len()))?;
        if !self.reads_as_korean(rest, partial) {
            return None;
        }
        let text = HangulComposer::compose(rest)?.text();
        Some(Decision {
            language: Language::Mixed,
            confidence: CONFIDENCE,
            output: format!("{unit}{text}"),
            explanation: format!("unit after a digit: {unit}+{text}"),
            runner_up: None,
        })
    }

    /// The best path with both languages when it beats the single-language readings by `margin`.
    pub fn split(&self, keys: &str, context: &DecisionContext) -> Option<Split> {
        self.analyze_from(keys, context, self.params.margin)
    }

    /// The best path with both languages and how far ahead of the single-language readings it
    /// is, whatever the margin (`gap` may be negative). None when no such path exists.
    ///
    /// A hyphen is no Korean key: Wi-Fieh is cut as WiFieh and the hyphens go back into the
    /// English stretches. One that would fall inside a Korean stretch rules the cut out.
    pub fn analyze(&self, keys: &str, context: &DecisionContext) -> Option<Split> {
        self.analyze_from(keys, context, f64::NEG_INFINITY)
    }

    /// `analyze` when the path is ahead by `min_gap` or more (the path is only read back then).
    fn analyze_from(&self, keys: &str, context: &DecisionContext, min_gap: f64) -> Option<Split> {
        if !keys.contains('-') {
            return self.analyze_letters(keys, context, min_gap);
        }
        let parts: Vec<&str> = keys.split('-').collect();
        if parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_alphabetic()))
        {
            return None;
        }
        let letters: String = parts.concat();
        let mut split = self.analyze_letters(&letters, context, min_gap)?;
        // Where each hyphen sat, counted in letters.
        let cuts: Vec<usize> = parts
            .iter()
            .scan(0, |at, p| {
                *at += p.len();
                Some(*at)
            })
            .take(parts.len() - 1)
            .collect();
        let mut start = 0;
        for piece in &mut split.pieces {
            let end = start + piece.keys.len();
            // The hyphens at or inside this stretch, counted from its start. One between two
            // stretches is kept by the English one: a Korean stretch just ignores it.
            let here: Vec<usize> = cuts
                .iter()
                .filter(|c| (start..=end).contains(c))
                .map(|c| c - start)
                .collect();
            if piece.kind == Kind::Korean {
                if here.iter().any(|h| (1..end - start).contains(h)) {
                    return None;
                }
            } else {
                for h in here.into_iter().rev() {
                    piece.keys.insert(h, '-');
                    piece.text.insert(h, '-');
                }
            }
            start = end;
        }
        Some(split)
    }

    fn analyze_letters(
        &self,
        keys: &str,
        context: &DecisionContext,
        min_gap: f64,
    ) -> Option<Split> {
        let n = keys.len();
        if !(MIN_KEYS..=MAX_KEYS).contains(&n) || !keys.bytes().all(|b| b.is_ascii_alphabetic()) {
            return None;
        }
        // A token that is already a listed English word or a clean Korean word is not worth a
        // search (it would only win by a bonus it already has). After a digit it may still be a
        // counter or a unit.
        let skip = |head: &Walk| {
            !context.after_digit
                && !context.before_digit
                && (self.is_english_word(keys, head)
                    || (self.reads_as_korean(keys, context.partial)
                        && !self.starts_with_word(head)))
        };
        let weak_margin = self.params.weak_margin;
        self.with_lattice(keys, context, skip, true, |_, table| {
            Self::read_path(keys, table, weak_margin, min_gap)
        })?
    }

    /// The best path into the end of `keys`, read back from the table.
    fn read_path(keys: &str, table: &Table, weak_margin: f64, min_gap: f64) -> Option<Split> {
        let n = keys.len();
        // (score less the weak margin, score, class, state) of the best path that switches.
        let mixed = CLASSES
            .iter()
            .flat_map(|c| [1, 3].map(|state| (c.index(), state)))
            .filter_map(|(class, state)| {
                let score = table.score(n, class, state)?;
                let weak = if state == 1 { weak_margin } else { 0.0 };
                Some((score - weak, score, class, state))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        let pure = CLASSES
            .iter()
            .filter(|c| **c != Class::EnLabel)
            .flat_map(|c| [0, 2].map(|state| table.score(n, c.index(), state)))
            .flatten()
            .max_by(f64::total_cmp);
        let gap = pure.map_or(f64::INFINITY, |p| mixed.0 - p);
        if gap < min_gap {
            return None;
        }

        let mut pieces: Vec<Piece> = Vec::new();
        let (mut pos, mut class, mut switched) = (n, mixed.2, mixed.3);
        while pos > 0 {
            let (from, prev_class, prev_switched) = table.from(pos, class, switched)?;
            let stretch = &keys[from..pos];
            let kind = CLASSES[class].kind();
            let text = match kind {
                Kind::English => stretch.to_string(),
                Kind::Korean => HangulComposer::compose(stretch)?.text(),
            };
            // Edges of one kind side by side are one stretch.
            match pieces.last_mut() {
                Some(last) if last.kind == kind => {
                    last.keys.insert_str(0, stretch);
                    last.text.insert_str(0, &text);
                }
                _ => pieces.push(Piece {
                    kind,
                    keys: stretch.to_string(),
                    text,
                }),
            }
            (pos, class, switched) = (from, prev_class, prev_switched);
        }
        pieces.reverse();
        Some(Split {
            pieces,
            score: mixed.1,
            gap,
        })
    }

    /// For `lab --trace`: the best path of each kind, edge by edge with its score (and every
    /// edge when `all`).
    pub fn trace(&self, keys: &str, context: &DecisionContext, all: bool) -> String {
        self.with_lattice(
            keys,
            context,
            |_| false,
            false,
            |edges, table| Self::trace_of(keys, edges, table, all),
        )
        .unwrap_or_else(|| "no edges".into())
    }

    fn trace_of(keys: &str, edges: &[Vec<Edge>], table: &Table, all: bool) -> String {
        use std::fmt::Write as _;
        let n = keys.len();
        let mut out = String::new();
        if all {
            for (i, list) in edges.iter().enumerate() {
                for e in list {
                    let _ = writeln!(
                        out,
                        "  {i}..{} {} {:?} {:.1}",
                        e.end(),
                        &keys[i..e.end()],
                        e.class,
                        e.score
                    );
                }
            }
        }
        let kinds = [("mixed ", 1), ("mixed+", 3), ("pure  ", 0), ("pure+ ", 2)];
        for (label, state) in kinds {
            let best = CLASSES
                .iter()
                .filter_map(|c| table.score(n, c.index(), state).map(|s| (s, c.index())))
                .max_by(|a, b| a.0.total_cmp(&b.0));
            let Some((total, mut class)) = best else {
                let _ = writeln!(out, "{label}: none");
                continue;
            };
            let _ = writeln!(out, "{label}: {total:.2}");
            let (mut pos, mut state) = (n, state);
            let mut path = Vec::new();
            while pos > 0 {
                let Some((from, prev_class, prev_state)) = table.from(pos, class, state) else {
                    break;
                };
                let score = edges[from]
                    .iter()
                    .find(|e| e.end() == pos && e.class == CLASSES[class])
                    .map_or(f64::NAN, |e| e.score);
                path.push(format!(
                    "{}[{:?} {score:.1}]",
                    &keys[from..pos],
                    CLASSES[class]
                ));
                (pos, class, state) = (from, prev_class, prev_state);
            }
            path.reverse();
            let _ = writeln!(out, "   {}", path.join(" "));
        }
        out
    }

    /// Runs `f` on the edges (grouped by where they start) and the table of `keys`; None when
    /// `skip` (asked with the walk of the whole token) says the token needs no search. Typing
    /// adds one key at a time and a key only changes what ends at the end, so what the last
    /// token searched already settled is kept: only the last `LOOKBACK` keys are searched again.
    fn with_lattice<R>(
        &self,
        keys: &str,
        context: &DecisionContext,
        skip: impl FnOnce(&Walk) -> bool,
        trim: bool,
        f: impl FnOnce(&[Vec<Edge>], &Table) -> R,
    ) -> Option<R> {
        LAST.with(|last| {
            let mut slot = last.borrow_mut();
            let mut cache = match slot.take() {
                Some(c) if c.params == self.params && c.after_digit == context.after_digit => c,
                _ => Cache {
                    params: self.params,
                    after_digit: context.after_digit,
                    keys: String::new(),
                    edges: Vec::new(),
                    table: Table::new(),
                    walks: Walks::new(),
                    fresh: Vec::new(),
                    floor: 0,
                },
            };
            let head = &keys.as_bytes()[..keys.len().min(EN_MAX)];
            self.english.walk_to(&mut cache.walks.head, head);
            if skip(&cache.walks.head) {
                *slot = Some(cache);
                return None;
            }
            let mut reached = shared(&cache.keys, keys);
            // Too little of what was dropped is left to go on from: search the token afresh.
            if cache.floor > 0 && reached < cache.floor + LOOKBACK {
                reached = 0;
                cache.keys.clear();
                cache.edges.clear();
                cache.table = Table::new();
                cache.floor = 0;
            }
            // A token that comes in at once (a paste, a story) is searched in pieces, as if it
            // were typed that fast: what is behind is dropped on the way, not all kept at the end.
            while keys.len() - reached > CHUNK {
                reached += CHUNK;
                self.extend(&mut cache, &keys[..reached], context, trim)?;
            }
            self.extend(&mut cache, keys, context, trim)?;
            let out = f(&cache.edges, &cache.table);
            *slot = Some(cache);
            Some(out)
        })
    }

    /// Searches `keys`, which shares its first keys with the token in `cache` (the last one
    /// searched): only what ends after those is built.
    fn extend(
        &self,
        cache: &mut Cache,
        keys: &str,
        context: &DecisionContext,
        trim: bool,
    ) -> Option<()> {
        // Everything that ends before the shared start of the two tokens is as it was.
        let kept = shared(&cache.keys, keys);
        let n = keys.len();
        cache.edges.resize(n, Vec::new());
        // A new key makes the last one no longer the end of the token, which changes the
        // edges that were built for the end: English ones (the letters' closing) and a lone
        // first consonant. A Korean edge is the same either way: keep it.
        let carry = kept < n;
        for list in &mut cache.edges[kept.saturating_sub(LOOKBACK).min(n)..] {
            list.retain(|e| {
                e.end() < kept
                    || (carry && e.end() == kept && e.class.kind() == Kind::Korean && !e.pending)
            });
        }
        self.korean_edges(keys, context, &mut cache.edges, &mut cache.walks, kept)?;
        self.english_edges(keys, context, &mut cache.edges, &mut cache.walks, kept);
        cache.table.truncate(kept);
        cache.table.resize(n + 1);
        self.search(n, &cache.edges, &mut cache.table, &mut cache.fresh, kept);
        cache.keys.clear();
        cache.keys.push_str(keys);
        // The search only looks back `LOOKBACK` keys: keep twice that and drop the rest,
        // each time a `LOOKBACK` more has gone by.
        if trim && n >= cache.floor + 3 * LOOKBACK {
            let floor = n - 2 * LOOKBACK;
            for list in &mut cache.edges[cache.floor..floor] {
                *list = Vec::new();
            }
            cache.table.trim(floor);
            cache.floor = floor;
        }
        Some(())
    }

    /// The edges that end at `min_end` or later (the earlier ones are in `edges` already).
    fn korean_edges(
        &self,
        keys: &str,
        context: &DecisionContext,
        edges: &mut [Vec<Edge>],
        walks: &mut Walks,
        min_end: usize,
    ) -> Option<()> {
        let bytes = keys.as_bytes();
        let n = bytes.len();
        let p = &self.params;
        for i in min_end.saturating_sub(LOOKBACK)..n {
            // A syllable starts on a consonant key; a stretch led by a vowel is all strays.
            if !DubeolsikLayout::starts_syllable(bytes[i] as char) {
                continue;
            }
            let walk = walks.korean_of(i);
            // Past `min_end` when the edges ending there were kept (see `with_lattice`).
            let carry = min_end < n;
            let first = (i + 1).max(min_end + usize::from(carry));
            walk.sync(&self.korean, &bytes[i..], first - i - usize::from(carry))?;
            for j in first..=n {
                if walk.keys.len() < j - i {
                    walk.push(&self.korean, bytes[j - 1])?;
                }
                let (verdict, dead) = walk.judge(&self.korean, j == n, context.partial);
                walk.judged = Some((walk.keys.len(), dead, matches!(verdict, Verdict::Stop)));
                let (syllables, pending, oov) = match verdict {
                    Verdict::Stop => break,
                    Verdict::Skip => continue,
                    Verdict::Fit {
                        syllables,
                        pending,
                        oov,
                    } => (syllables, pending, oov),
                };
                let caps = p.cap_penalty * walk.capitals as f64;
                if pending {
                    // The next syllable's first consonant, alone, while the token is still typed.
                    if syllables == 0 {
                        edges[i].push(Edge {
                            pending: true,
                            ..Edge::new(j, Class::KoOov, PENDING_CONSONANT_LOGP)
                        });
                    }
                    continue;
                }
                if syllables == 0 {
                    continue;
                }
                walk.narrow_lines(&self.korean);
                let word = walk.word.as_str();
                let ending_like = is_ending(word);
                // A lone syllable that is a word of its own, said often (좀, 해).
                let frequent = syllables == 1 && self.is_frequent_syllable(word);
                let mut push = |class, score: f64| {
                    edges[i].push(Edge {
                        ending_like: ending_like && class == Class::KoOov,
                        // An ending of two syllables or more (해서, 했어) is a verb form, not a stray.
                        known: frequent || (class == Class::KoEnding && syllables >= 2),
                        ..Edge::new(j, class, score - caps)
                    });
                };
                if context.after_digit && i == 0 && is_counter_word(word) {
                    push(Class::KoWord, p.digit_bonus);
                }
                if is_single_letter_particle(word) {
                    push(Class::KoCase, p.ending_logp);
                } else if ending_like {
                    push(Class::KoEnding, p.ending_logp);
                }
                if syllables == 1 {
                    let lead = if i == 0 { p.lead_ko } else { 0.0 };
                    push(Class::KoOov, oov - p.ko_oov + lead);
                } else if let Some(logp) = self.korean.word_log_probability_in(walk.lines, word) {
                    push(Class::KoWord, logp);
                }
            }
        }
        Some(())
    }

    /// Whether a lone syllable is a Korean word said often (`LONE_KNOWN_LOGP`); a few hundred
    /// syllables come up again and again, so each is looked up once.
    fn is_frequent_syllable(&self, word: &str) -> bool {
        let Some(slot) = word
            .chars()
            .next()
            .and_then(|c| (c as usize).checked_sub('가' as usize))
            .filter(|slot| *slot < SYLLABLES)
        else {
            return false;
        };
        FREQUENT.with(|cache| {
            let mut cache = cache.borrow_mut();
            let (word_at, bit) = (slot / 64, 1u64 << (slot % 64));
            if cache[word_at] & bit == 0 {
                cache[word_at] |= bit;
                let logp = self.korean.word_log_probability(word);
                if logp.is_some_and(|logp| logp > LONE_KNOWN_LOGP) {
                    cache[SYLLABLE_WORDS + word_at] |= bit;
                }
            }
            cache[SYLLABLE_WORDS + word_at] & bit != 0
        })
    }

    fn english_edges(
        &self,
        keys: &str,
        context: &DecisionContext,
        edges: &mut [Vec<Edge>],
        walks: &mut Walks,
        min_end: usize,
    ) {
        let n = keys.len();
        let p = &self.params;
        for i in min_end.saturating_sub(LOOKBACK)..n {
            // A start whose longest stretch ended before `min_end` has nothing to add.
            if i + EN_MAX < min_end.max(i + 1) {
                continue;
            }
            let walk = walks.of(i);
            self.english
                .walk_to(walk, &keys.as_bytes()[i..n.min(i + EN_MAX)]);
            for j in (i + 1).max(min_end)..=n.min(i + EN_MAX) {
                let seg = &keys[i..j];
                let len = j - i;
                if i == 0 && is_label(seg) {
                    edges[i].push(Edge::new(j, Class::EnLabel, p.label_logp));
                }
                let unit = context.after_digit
                    && i == 0
                    && UNITS.contains(&seg.to_ascii_lowercase().as_str());
                // Letters right before a digit are a name (p95, v0.9.7, k8s), whatever they spell.
                let tail = context.before_digit && j == n;
                // A lone letter is English only as a label in front (n제), a unit (3x) or next to
                // a digit, never mid-token.
                // One-letter English words (a, I) are words wherever they are.
                let one_word = j - i == 1 && matches!(seg, "a" | "A" | "I");
                if j - i < 2 && !unit && !tail && !one_word {
                    continue;
                }
                let bonus = if unit || (tail && !one_word) {
                    p.digit_bonus
                } else {
                    0.0
                };
                let listed = if one_word {
                    self.english.walk_word_log_probability(walk, len)
                } else {
                    (len >= 2).then(|| self.word_logp(walk, len)).flatten()
                };
                if let Some(logp) = listed {
                    edges[i].push(Edge::new(j, Class::EnWord, logp + bonus));
                    continue;
                }
                let closed = !((context.partial || tail) && j == n);
                let letters = self.english.walk_letters(walk, len, closed);
                edges[i].push(if unit {
                    Edge::new(j, Class::EnWord, letters + bonus)
                } else {
                    Edge::new(j, Class::EnOov, letters - p.en_oov + bonus)
                });
            }
        }
    }

    /// A listed English word of an edge's length: any from three letters, an everyday one of two.
    fn word_logp(&self, walk: &Walk, len: usize) -> Option<f64> {
        if len == 2 && !self.english.walk_is_common(walk, len) {
            return None;
        }
        self.english.walk_word_log_probability(walk, len)
    }

    /// The cost of the step from an edge of class `prev` to `edge`, and whether it changes the
    /// language. None: the two can't be side by side.
    fn join(&self, prev: Class, edge: &Edge, n: usize) -> Option<(f64, bool)> {
        let p = &self.params;
        if edge.class == Class::EnLabel || (edge.pending && prev.kind() != Kind::Korean) {
            return None;
        }
        // A label names the noun that ends the token (n제, x축) or takes a case particle (x에).
        if prev == Class::EnLabel {
            return match edge.class {
                Class::KoCase => Some((p.switch_cost, true)),
                Class::KoWord | Class::KoOov if edge.end() == n && !edge.ending_like => {
                    Some((p.switch_cost - p.label_noun, true))
                }
                _ => None,
            };
        }
        if prev.kind() != edge.class.kind() {
            let enter = match edge.class {
                Class::KoOov => p.ko_enter,
                Class::EnOov => p.en_enter,
                _ => 0.0,
            };
            return Some((p.switch_cost + enter, true));
        }
        let cost = match (prev, edge.class) {
            // Each syllable of such a run already paid `ko_oov`.
            (Class::KoOov, Class::KoOov) => 0.0,
            (_, class) if class.kind() == Kind::Korean => p.ko_join,
            _ => p.en_join,
        };
        Some((cost, false))
    }

    /// The best path into every position, per class of its last edge and whether it has switched.
    /// Positions before `settled` are already in `table`: only the paths that end at `settled`
    /// or later are searched again.
    fn search(
        &self,
        n: usize,
        edges: &[Vec<Edge>],
        table: &mut Table,
        fresh: &mut Vec<u32>,
        settled: usize,
    ) {
        for edge in &edges[0] {
            // A particle or ending needs a word before it to attach to.
            if edge.end() < settled || matches!(edge.class, Class::KoCase | Class::KoEnding) {
                continue;
            }
            let state = usize::from(edge.class == Class::KoWord) << 1;
            table.offer(
                edge.end(),
                (edge.class.index(), state),
                edge.score,
                (0, START, 0),
            );
        }
        for pos in settled.saturating_sub(LOOKBACK).max(1)..n {
            fresh.clear();
            fresh.extend(
                (0..edges[pos].len() as u32).filter(|k| edges[pos][*k as usize].end() >= settled),
            );
            if fresh.is_empty() {
                continue;
            }
            for prev in CLASSES {
                for switched in 0..STATES {
                    let Some(score) = table.score(pos, prev.index(), switched) else {
                        continue;
                    };
                    for edge in fresh.iter().map(|k| &edges[pos][*k as usize]) {
                        let Some((cost, changes)) = self.join(prev, edge, n) else {
                            continue;
                        };
                        let strong = matches!(edge.class, Class::KoWord | Class::KoCase)
                            || edge.known
                            || prev == Class::EnLabel;
                        let now = (switched & 1 | usize::from(changes))
                            | (switched & 2)
                            | usize::from(strong) << 1;
                        let total = score + edge.score - cost;
                        table.offer(
                            edge.end(),
                            (edge.class.index(), now),
                            total,
                            (pos, prev.index(), switched),
                        );
                    }
                }
            }
        }
    }

    /// Whether a listed English word of at least `LEAD_WORD_KEYS` letters leads the token
    /// (slack in slackdptj, which also reads as Korean): the search may still prefer it.
    fn starts_with_word(&self, head: &Walk) -> bool {
        (LEAD_WORD_KEYS..head.len()).any(|end| self.english.walk_is_known(head, end))
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
        COMPOSER.with(|state| {
            let mut state = state.borrow_mut();
            state.clear();
            for key in keys.chars() {
                let Some(jamo) = DubeolsikLayout::jamo(key) else {
                    return false;
                };
                state.push(jamo);
            }
            let (mut syllables, mut reads, mut tail) = (0usize, true, false);
            state.for_each_unit(|unit| {
                // Nothing may follow the next syllable's first consonant.
                reads &= !tail;
                match unit {
                    Unit::Syllable(s) => {
                        reads &= self.korean.syllable_log_probability(s) > UNKNOWN_SYLLABLE_LOGP;
                        syllables += 1;
                    }
                    // The next syllable's first consonant, while the token is still being typed.
                    Unit::Jamo(j) if partial && is_consonant(j) => tail = true,
                    Unit::Jamo(_) => reads = false,
                }
            });
            reads && syllables > 0
        })
    }

    /// `head` is the walk of the token's first `EN_MAX` keys: a longer token is asked afresh.
    fn is_english_word(&self, keys: &str, head: &Walk) -> bool {
        if keys.len() <= EN_MAX {
            self.english.walk_is_known(head, keys.len())
        } else {
            self.english.is_known_word(keys)
        }
    }
}

/// How many keys two tokens start with in common.
fn shared(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(a, b)| a == b).count()
}

/// One letter, or 2-4 capitals: what is typed in front of a word to name it (n제, A급, QR코드).
pub(crate) fn is_label(seg: &str) -> bool {
    seg.len() == 1 || ((2..=4).contains(&seg.len()) && seg.chars().all(|c| c.is_ascii_uppercase()))
}

/// A counter (년월일…) alone or with an ending: 2024년, 3개를.
pub(crate) fn is_counter_word(word: &str) -> bool {
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
        let lattice = CodeSwitch::new(EnglishModel::shared(), Params::default());
        lattice.split(keys, context).map(|s| s.text())
    }

    fn split(keys: &str) -> Option<String> {
        cut(keys, &DecisionContext::default())
    }

    /// What the lattice says about `keys` with nothing remembered from the token before.
    fn fresh(
        lattice: &CodeSwitch,
        keys: &str,
        context: &DecisionContext,
    ) -> Option<(String, f64, f64)> {
        LAST.with(|last| *last.borrow_mut() = None);
        lattice
            .analyze(keys, context)
            .map(|s| (s.text(), s.score, s.gap))
    }

    #[test]
    fn extending_or_trimming_the_last_token_equals_searching_it_afresh() {
        let lattice = CodeSwitch::new(EnglishModel::shared(), Params::default());
        for whole in [
            "dhsmfgithubdppushgoTsmsepbuildrkRowuTdj",
            "greenappleshfowhgekkakaotalkaptlswjxcnrQRzhem",
            "dhsmfgithubdppushgoTsmsepbuildrkRowuTdj".repeat(4).as_str(),
            "dhsmfgithubdppushgoTsmsepbuildrkRowuTdj".repeat(9).as_str(),
        ] {
            for partial in [true, false] {
                let context = DecisionContext {
                    partial,
                    ..Default::default()
                };
                let lengths: Vec<usize> = (2..=whole.len()).chain((2..whole.len()).rev()).collect();
                LAST.with(|last| *last.borrow_mut() = None);
                for len in lengths {
                    let keys = &whole[..len];
                    let cached = lattice
                        .analyze(keys, &context)
                        .map(|s| (s.text(), s.score, s.gap));
                    let afresh = fresh(&lattice, keys, &context);
                    assert_eq!(cached, afresh, "{keys} partial={partial}");
                    // Put back what the cache held, to go on from it.
                    lattice.analyze(keys, &context);
                }
            }
        }
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
            ("postgresdml", "postgres의"),
            ("postgresdptj", "postgres에서"),
            ("postgresgoTdj", "postgres했어"),
            ("greenappleshfowhgek", "greenapple노래좋다"),
            (
                "dhsmfgithubdppushgoTsmsepbuildrkRowuTdj",
                "오늘github에push했는데build가깨졌어",
            ),
        ] {
            assert_eq!(split(keys).as_deref(), Some(text), "{keys}");
        }
    }

    #[test]
    fn splits_a_long_unspaced_sentence() {
        let keys = "dkssudgktpdygithubdptjcommitgktpdy";
        assert_eq!(
            split(keys).as_deref(),
            Some("안녕하세요github에서commit하세요")
        );
        for (keys, text) in [
            (
                "dkssudgktpdyhellowjsmsrlatpwnsdlfkrhgkrhdeveloperfhdlfgkrhdlTtmqslek",
                "안녕하세요hello저는김세준이라고하고developer로일하고있습니다",
            ),
            (
                "ekdidgksprojectgkrhdlTrhdkvdmfhehdevelopergkftodrkrdlqslek",
                "다양한project하고있고앞으로도developer할생각입니다",
            ),
        ] {
            assert_eq!(split(keys).as_deref(), Some(text), "{keys}");
        }
        assert_eq!(
            split(&keys.repeat(4)).as_deref(),
            Some("안녕하세요github에서commit하세요".repeat(4).as_str())
        );
    }

    #[test]
    fn a_hyphen_stays_inside_the_english_stretch() {
        for (keys, text) in [
            ("Wi-Fieh", "Wi-Fi도"),
            ("Wi-FiRkwl", "Wi-Fi까지"),
            ("e-mailfmf", "e-mail를"),
            ("wi-fieh", "wi-fi도"),
            ("dkssud-hello", "안녕-hello"),
        ] {
            assert_eq!(split(keys).as_deref(), Some(text), "{keys}");
        }
        for keys in ["-Fieh", "Wi-", "Wi--Fieh", "dkssud-dkssud", "hello-world"] {
            assert_eq!(split(keys), None, "{keys}");
        }
    }

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
