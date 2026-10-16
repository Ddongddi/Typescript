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
use crate::lang::korean::{KoreanModel, PENDING_CONSONANT_LOGP, UNKNOWN_SYLLABLE_LOGP};
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

/// Best path into a position: (score, where from, its class, whether it had switched).
type Cell = Option<(f64, u32, u8, u8)>;
/// Per position, per class of the last edge, per state of the path: bit 0 is whether it has
/// switched language, bit 1 whether a Korean word, case particle or label noun is on it.
type Table = Vec<[[Cell; STATES]; 7]>;
const STATES: usize = 4;

/// The lattice of the token searched last, to extend by the next key.
struct Cache {
    params: Params,
    after_digit: bool,
    keys: String,
    edges: Vec<Vec<Edge>>,
    table: Table,
    walks: Walks,
}

/// The English word list and letter model walked along the letters from each start key: the
/// first key's walk (the whole token is asked about it) and one per start of the last
/// `EN_MAX` keys, in slots by start.
struct Walks {
    head: Walk,
    ring: Vec<(usize, Walk)>,
}

impl Walks {
    fn new() -> Self {
        Self {
            head: Walk::default(),
            ring: (0..EN_MAX).map(|_| (0, Walk::default())).collect(),
        }
    }

    /// The walk of the stretches that start at key `i` (empty if it was another start's).
    fn of(&mut self, i: usize) -> &mut Walk {
        if i == 0 {
            return &mut self.head;
        }
        let slot = &mut self.ring[i % EN_MAX];
        if slot.0 != i {
            *slot = (i, Walk::default());
        }
        &mut slot.1
    }
}

const EMPTY_ROW: [[Cell; STATES]; 7] = [[None; STATES]; 7];

thread_local! {
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
        self.analyze(keys, context)
            .filter(|s| s.gap >= self.params.margin)
    }

    /// The best path with both languages and how far ahead of the single-language readings it
    /// is, whatever the margin (`gap` may be negative). None when no such path exists.
    ///
    /// A hyphen is no Korean key: Wi-Fieh is cut as WiFieh and the hyphens go back into the
    /// English stretches. One that would fall inside a Korean stretch rules the cut out.
    pub fn analyze(&self, keys: &str, context: &DecisionContext) -> Option<Split> {
        if !keys.contains('-') {
            return self.analyze_letters(keys, context);
        }
        let parts: Vec<&str> = keys.split('-').collect();
        if parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_alphabetic()))
        {
            return None;
        }
        let letters: String = parts.concat();
        let mut split = self.analyze_letters(&letters, context)?;
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

    fn analyze_letters(&self, keys: &str, context: &DecisionContext) -> Option<Split> {
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
        self.with_lattice(keys, context, skip, |_, table| {
            Self::read_path(keys, table, weak_margin)
        })?
    }

    /// The best path into the end of `keys`, read back from the table.
    fn read_path(keys: &str, table: &Table, weak_margin: f64) -> Option<Split> {
        let n = keys.len();
        // (score less the weak margin, score, class, state) of the best path that switches.
        let mixed = CLASSES
            .iter()
            .flat_map(|c| [1, 3].map(|state| (c.index(), state)))
            .filter_map(|(class, state)| {
                let score = table[n][class][state]?.0;
                let weak = if state == 1 { weak_margin } else { 0.0 };
                Some((score - weak, score, class, state))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        let pure = CLASSES
            .iter()
            .filter(|c| **c != Class::EnLabel)
            .flat_map(|c| [0, 2].map(|state| table[n][c.index()][state]))
            .flatten()
            .map(|cell| cell.0)
            .max_by(f64::total_cmp);
        let gap = pure.map_or(f64::INFINITY, |p| mixed.0 - p);

        let mut pieces: Vec<Piece> = Vec::new();
        let (mut pos, mut class, mut switched) = (n, mixed.2, mixed.3);
        while pos > 0 {
            let (_, from, prev_class, prev_switched) = table[pos][class][switched]?;
            let (from, prev_class, prev_switched) = (
                from as usize,
                usize::from(prev_class),
                usize::from(prev_switched),
            );
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
                .filter_map(|c| table[n][c.index()][state].map(|cell| (cell.0, c.index())))
                .max_by(|a, b| a.0.total_cmp(&b.0));
            let Some((total, mut class)) = best else {
                let _ = writeln!(out, "{label}: none");
                continue;
            };
            let _ = writeln!(out, "{label}: {total:.2}");
            let (mut pos, mut state) = (n, state);
            let mut path = Vec::new();
            while pos > 0 {
                let Some((_, from, prev_class, prev_state)) = table[pos][class][state] else {
                    break;
                };
                let from = from as usize;
                let score = edges[from]
                    .iter()
                    .find(|e| e.end() == pos && e.class == CLASSES[class])
                    .map_or(f64::NAN, |e| e.score);
                path.push(format!(
                    "{}[{:?} {score:.1}]",
                    &keys[from..pos],
                    CLASSES[class]
                ));
                (pos, class, state) = (from, usize::from(prev_class), usize::from(prev_state));
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
                    table: Vec::new(),
                    walks: Walks::new(),
                },
            };
            let head = &keys.as_bytes()[..keys.len().min(EN_MAX)];
            self.english.walk_to(&mut cache.walks.head, head);
            if skip(&cache.walks.head) {
                *slot = Some(cache);
                return None;
            }
            // Everything that ends before the shared start of the two tokens is as it was.
            let kept = cache
                .keys
                .bytes()
                .zip(keys.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            let n = keys.len();
            cache.edges.resize(n, Vec::new());
            for list in &mut cache.edges[kept.saturating_sub(LOOKBACK).min(n)..] {
                list.retain(|e| e.end() < kept);
            }
            self.korean_edges(keys, context, &mut cache.edges, kept)?;
            self.english_edges(keys, context, &mut cache.edges, &mut cache.walks, kept);
            cache.table.truncate(kept);
            cache.table.resize(n + 1, EMPTY_ROW);
            self.search(n, &cache.edges, &mut cache.table, kept);
            cache.keys.clear();
            cache.keys.push_str(keys);
            let out = f(&cache.edges, &cache.table);
            *slot = Some(cache);
            Some(out)
        })
    }

    /// The edges that end at `min_end` or later (the earlier ones are in `edges` already).
    fn korean_edges(
        &self,
        keys: &str,
        context: &DecisionContext,
        edges: &mut [Vec<Edge>],
        min_end: usize,
    ) -> Option<()> {
        let bytes = keys.as_bytes();
        let n = bytes.len();
        let p = &self.params;
        let mut word = String::new();
        for i in min_end.saturating_sub(LOOKBACK)..n {
            // A syllable starts on a consonant key; a stretch led by a vowel is all strays.
            if !DubeolsikLayout::starts_syllable(bytes[i] as char) {
                continue;
            }
            let mut state = ComposerState::default();
            let mut dead = false;
            for j in i + 1..=n {
                state.push(DubeolsikLayout::jamo(bytes[j - 1] as char)?);
                let (mut syllables, mut feasible, mut pending) = (0usize, true, false);
                let mut oov = 0.0;
                word.clear();
                state.for_each_unit(|unit| {
                    // A stray before a later unit stays a stray however long the stretch gets.
                    dead |= !feasible;
                    // Only the last unit may be the next syllable's first consonant.
                    feasible &= !pending;
                    match unit {
                        Unit::Syllable(s) => {
                            oov += self.korean.plausible_log_probability(s);
                            syllables += 1;
                            word.push(s);
                        }
                        Unit::Jamo(jamo) if context.partial && j == n && is_consonant(jamo) => {
                            pending = true;
                        }
                        Unit::Jamo(_) => feasible = false,
                    }
                });
                if !feasible {
                    if dead {
                        break;
                    }
                    continue;
                }
                if syllables > KO_MAX_SYLLABLES {
                    break;
                }
                if j < min_end {
                    continue;
                }
                let capitals = keys[i..j]
                    .chars()
                    .filter(|c| c.is_uppercase() && !DubeolsikLayout::is_korean_shift_key(*c))
                    .count();
                let caps = p.cap_penalty * capitals as f64;
                let ending_like = is_ending(&word);
                // A lone syllable that is a word of its own, said often (좀, 해).
                let frequent = syllables == 1
                    && self
                        .korean
                        .word_log_probability(&word)
                        .is_some_and(|logp| logp > LONE_KNOWN_LOGP);
                let mut push = |class, score: f64| {
                    edges[i].push(Edge {
                        ending_like: ending_like && class == Class::KoOov,
                        // An ending of two syllables or more (해서, 했어) is a verb form, not a stray.
                        known: frequent || (class == Class::KoEnding && syllables >= 2),
                        ..Edge::new(j, class, score - caps)
                    });
                };
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
                if context.after_digit && i == 0 && is_counter_word(&word) {
                    push(Class::KoWord, p.digit_bonus);
                }
                if is_single_letter_particle(&word) {
                    push(Class::KoCase, p.ending_logp);
                } else if is_ending(&word) {
                    push(Class::KoEnding, p.ending_logp);
                }
                if syllables == 1 {
                    let lead = if i == 0 { p.lead_ko } else { 0.0 };
                    push(Class::KoOov, oov - p.ko_oov + lead);
                } else if let Some(logp) = self.korean.word_log_probability(&word) {
                    push(Class::KoWord, logp);
                }
            }
        }
        Some(())
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
    fn search(&self, n: usize, edges: &[Vec<Edge>], table: &mut Table, settled: usize) {
        for edge in &edges[0] {
            // A particle or ending needs a word before it to attach to.
            if edge.end() < settled || matches!(edge.class, Class::KoCase | Class::KoEnding) {
                continue;
            }
            let state = usize::from(edge.class == Class::KoWord) << 1;
            let slot = &mut table[edge.end()][edge.class.index()][state];
            if slot.is_none_or(|old| edge.score > old.0) {
                *slot = Some((edge.score, 0, u8::MAX, 0));
            }
        }
        // The edges of one start that end at `settled` or later, in order (reused).
        let mut fresh: Vec<&Edge> = Vec::new();
        for pos in settled.saturating_sub(LOOKBACK).max(1)..n {
            fresh.clear();
            fresh.extend(edges[pos].iter().filter(|e| e.end() >= settled));
            if fresh.is_empty() {
                continue;
            }
            for prev in CLASSES {
                for switched in 0..STATES {
                    let Some((score, _, _, _)) = table[pos][prev.index()][switched] else {
                        continue;
                    };
                    for edge in fresh.iter().copied() {
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
                        let slot = &mut table[edge.end()][edge.class.index()][now];
                        if slot.is_none_or(|old| total > old.0) {
                            *slot = Some((total, pos as u32, prev.index() as u8, switched as u8));
                        }
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

    /// `head` is the walk of the token's first `EN_MAX` keys: a longer token is asked afresh.
    fn is_english_word(&self, keys: &str, head: &Walk) -> bool {
        if keys.len() <= EN_MAX {
            self.english.walk_is_known(head, keys.len())
        } else {
            self.english.is_known_word(keys)
        }
    }
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
