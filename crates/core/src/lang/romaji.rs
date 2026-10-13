//! Romaji → hiragana, following the conventions of the macOS and Microsoft Japanese input
//! methods (nn/n' → ん, doubled consonants → っ, x/l small kana, and - → ー). Letters that
//! can't become kana are kept and counted: like stray jamo for Hangul, they are the strongest
//! sign the user wasn't typing Japanese. Port of RomajiComposer.swift.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// The result of composing romaji keys: kana with anything that didn't become kana.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposeResult {
    /// Kana with any unconvertible letters left in place (こんにc while typing).
    pub text: String,
    /// Letters in the middle or (when finished) at the end that never became kana.
    pub stray_letters: usize,
    /// Letters at the end still waiting to become kana ("k", "ky", "n"); only when partial.
    pub pending: String,
    /// Number of kana produced.
    pub kana_count: usize,
}

impl ComposeResult {
    pub fn is_clean(&self) -> bool {
        self.stray_letters == 0 && self.pending.is_empty()
    }
}

pub struct RomajiComposer;

impl RomajiComposer {
    /// - Parameter partial: the word is still being typed, so a trailing "k" or "n"
    ///   is waiting for its vowel rather than stray.
    pub fn compose(keys: &str, partial: bool) -> ComposeResult {
        let chars: Vec<char> = keys.to_lowercase().chars().collect();
        let mut out = String::new();
        let mut stray = 0usize;
        let mut kana = 0usize;
        let mut pending = String::new();
        let mut i = 0usize;

        fn emit(out: &mut String, kana: &mut usize, s: &str) {
            out.push_str(s);
            *kana += s.chars().count();
        }

        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();

            // Digits are full-width, and so is the punctuation inside a number (０３－１２３４, ３．５).
            if let Some(full) = full_width_digit(c) {
                emit(&mut out, &mut kana, &full.to_string());
                i += 1;
                continue;
            }
            if i > 0 && chars[i - 1].is_ascii_digit() {
                if let Some(full) = number_punctuation(c) {
                    emit(&mut out, &mut kana, &full.to_string());
                    i += 1;
                    continue;
                }
            }
            if c == 'n' {
                if let Some(next) = next {
                    if next == '\'' {
                        emit(&mut out, &mut kana, "ん");
                        i += 2;
                        continue;
                    }
                    if next == 'n' {
                        // "nni" → んに, "nn" → ん
                        let after = chars.get(i + 2).copied();
                        emit(&mut out, &mut kana, "ん");
                        i += if after.is_some_and(|a| VOWELS.contains(&a) || a == 'y') {
                            1
                        } else {
                            2
                        };
                        continue;
                    }
                    if !VOWELS.contains(&next) && next != 'y' {
                        emit(&mut out, &mut kana, "ん");
                        i += 1;
                        continue;
                    }
                } else {
                    if partial {
                        pending = "n".to_string();
                    } else {
                        emit(&mut out, &mut kana, "ん");
                    }
                    i += 1;
                    continue;
                }
            } else if next == Some(c) && CONSONANTS.contains(&c) {
                emit(&mut out, &mut kana, "っ");
                i += 1;
                continue;
            } else if c == 't' && next == Some('c') && chars.get(i + 2) == Some(&'h') {
                emit(&mut out, &mut kana, "っ");
                i += 1;
                continue; // matcha: "tch" → っch
            }

            let mut matched = false;
            let max = std::cmp::min(4, chars.len() - i);
            for length in (1..=max).rev() {
                let key: String = chars[i..i + length].iter().collect();
                if let Some(k) = TABLE.get(key.as_str()) {
                    emit(&mut out, &mut kana, k);
                    i += length;
                    matched = true;
                    break;
                }
            }
            if matched {
                continue;
            }

            let rest: String = chars[i..].iter().collect();
            if partial && PREFIXES.contains(&rest) {
                pending = rest;
                break;
            }
            out.push(c);
            stray += 1;
            i += 1;
        }
        ComposeResult {
            text: format!("{out}{pending}"),
            stray_letters: stray,
            pending,
            kana_count: kana,
        }
    }

    /// Hiragana → katakana (for F7-style katakana output and loanwords).
    pub fn katakana(hiragana: &str) -> String {
        hiragana
            .chars()
            .map(|c| {
                let v = c as u32;
                if (0x3041..=0x3096).contains(&v) {
                    char::from_u32(v + 0x60).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect()
    }

    /// A spelling that composes back to `kana` (し → "si", ん → "n'", っ → "ltu"), or None if
    /// some character has none. For keys rewritten after Delete took off a whole kana.
    pub fn romaji(kana: &str) -> Option<String> {
        let mut out = String::new();
        for ch in kana.chars() {
            out.push_str(SPELLING.get(&ch)?);
        }
        Some(out)
    }

    /// Whether a key is one of the symbols the composer maps (、。？！ー「」・…).
    pub fn is_symbol_key(c: char) -> bool {
        SYMBOLS.contains_key(&c)
    }

    /// Non-letters that belong inside a Japanese phrase: ん (n'), ー, and the full-width symbols
    /// (、。？！「」・…).
    pub fn is_in_phrase_symbol(c: char) -> bool {
        c == '\'' || SYMBOLS.contains_key(&c)
    }

    /// Sentence punctuation (、。？！「」・…), but not ー, which is part of a word.
    pub fn is_punctuation(c: char) -> bool {
        PUNCTUATION.contains(&c)
    }
}

/// The shortest table entry for each single kana (ties: alphabetical); ん as "n'" so a
/// vowel typed after it can't take it over (n'a is んあ, nna would be んな).
static SPELLING: LazyLock<HashMap<char, String>> = LazyLock::new(|| {
    let mut s: HashMap<char, String> = HashMap::new();
    s.insert('ん', "n'".to_string());
    for (key, value) in TABLE.iter() {
        let mut chars = value.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            continue;
        };
        if let Some(old) = s.get(&ch) {
            let (old_len, key_len) = (old.chars().count(), key.chars().count());
            if old_len < key_len || (old_len == key_len && old.as_str() < key.as_str()) {
                continue;
            }
        }
        if ch != 'ん' {
            s.insert(ch, key.clone());
        }
    }
    s
});

/// Symbols as Japanese IMEs type them by default: full-width, with [ ] / ~ - as
/// 「」・〜ー. Not ' (it ends ん in kan'i), and digits and spaces stay half-width.
static SYMBOLS: LazyLock<HashMap<char, &'static str>> =
    LazyLock::new(|| SYMBOL_TABLE.into_iter().collect());

/// What the symbols become, except ー, which is part of a word (らーめん) rather than punctuation.
static PUNCTUATION: LazyLock<HashSet<char>> = LazyLock::new(|| {
    SYMBOLS
        .values()
        .filter_map(|s| s.chars().next())
        .filter(|c| *c != 'ー')
        .collect()
});

/// Every proper prefix of a table entry: what a word may end in while still being typed.
static PREFIXES: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let mut p: HashSet<String> = HashSet::new(); // "n" is not in the table
    for key in TABLE.keys() {
        let n = key.chars().count();
        if n > 1 {
            for length in 1..n {
                p.insert(key.chars().take(length).collect());
            }
        }
    }
    p
});

/// 0–9 as ０–９, for a Japanese phrase with a number in it.
fn full_width_digit(c: char) -> Option<char> {
    c.is_ascii_digit()
        .then(|| char::from_u32(c as u32 + 0xFEE0).unwrap())
}

/// After a digit, - . , / belong to the number (addresses, phone numbers, dates), not ー。、・.
fn number_punctuation(c: char) -> Option<char> {
    matches!(c, '-' | '.' | ',' | '/').then(|| char::from_u32(c as u32 + 0xFEE0).unwrap())
}

static VOWELS: LazyLock<HashSet<char>> =
    LazyLock::new(|| ['a', 'i', 'u', 'e', 'o'].into_iter().collect());
static CONSONANTS: LazyLock<HashSet<char>> =
    LazyLock::new(|| "bcdfghjklmpqrstvwxyz".chars().collect());

/// The romaji table the composer matches, longest entry first. Built once: the base entries,
/// the symbols, and the contracted sounds (きゃ, ぎゅ, みょ…).
static TABLE: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let mut t: HashMap<String, String> = HashMap::with_capacity(300);
    for (k, v) in BASE_TABLE {
        t.insert(k.to_string(), v.to_string());
    }
    for (symbol, full) in SYMBOL_TABLE {
        t.insert(symbol.to_string(), full.to_string());
    }
    for (consonant, i) in CONTRACTED_ROWS {
        for (vowel, small) in CONTRACTED_VOWELS {
            t.insert(format!("{consonant}{vowel}"), format!("{i}{small}"));
        }
    }
    t
});

/// As macOS and MS-IME accept them (not ca/ci/cu: they made English words read as romaji).
const BASE_TABLE: [(&str, &str); 152] = [
    ("a", "あ"),
    ("i", "い"),
    ("u", "う"),
    ("e", "え"),
    ("o", "お"),
    ("ka", "か"),
    ("ki", "き"),
    ("ku", "く"),
    ("ke", "け"),
    ("ko", "こ"),
    ("ga", "が"),
    ("gi", "ぎ"),
    ("gu", "ぐ"),
    ("ge", "げ"),
    ("go", "ご"),
    ("sa", "さ"),
    ("si", "し"),
    ("su", "す"),
    ("se", "せ"),
    ("so", "そ"),
    ("za", "ざ"),
    ("zi", "じ"),
    ("zu", "ず"),
    ("ze", "ぜ"),
    ("zo", "ぞ"),
    ("ta", "た"),
    ("ti", "ち"),
    ("tu", "つ"),
    ("te", "て"),
    ("to", "と"),
    ("da", "だ"),
    ("di", "ぢ"),
    ("du", "づ"),
    ("de", "で"),
    ("do", "ど"),
    ("na", "な"),
    ("ni", "に"),
    ("nu", "ぬ"),
    ("ne", "ね"),
    ("no", "の"),
    ("ha", "は"),
    ("hi", "ひ"),
    ("hu", "ふ"),
    ("he", "へ"),
    ("ho", "ほ"),
    ("ba", "ば"),
    ("bi", "び"),
    ("bu", "ぶ"),
    ("be", "べ"),
    ("bo", "ぼ"),
    ("pa", "ぱ"),
    ("pi", "ぴ"),
    ("pu", "ぷ"),
    ("pe", "ぺ"),
    ("po", "ぽ"),
    ("ma", "ま"),
    ("mi", "み"),
    ("mu", "む"),
    ("me", "め"),
    ("mo", "も"),
    ("ya", "や"),
    ("yi", "い"),
    ("yu", "ゆ"),
    ("ye", "いぇ"),
    ("yo", "よ"),
    ("ra", "ら"),
    ("ri", "り"),
    ("ru", "る"),
    ("re", "れ"),
    ("ro", "ろ"),
    ("wa", "わ"),
    ("wi", "うぃ"),
    ("wu", "う"),
    ("we", "うぇ"),
    ("wo", "を"),
    ("fa", "ふぁ"),
    ("fi", "ふぃ"),
    ("fu", "ふ"),
    ("fe", "ふぇ"),
    ("fo", "ふぉ"),
    ("va", "ゔぁ"),
    ("vi", "ゔぃ"),
    ("vu", "ゔ"),
    ("ve", "ゔぇ"),
    ("vo", "ゔぉ"),
    ("qa", "くぁ"),
    ("qi", "くぃ"),
    ("qu", "く"),
    ("qe", "くぇ"),
    ("qo", "くぉ"),
    ("shi", "し"),
    ("chi", "ち"),
    ("tsu", "つ"),
    ("ji", "じ"),
    ("sha", "しゃ"),
    ("shu", "しゅ"),
    ("she", "しぇ"),
    ("sho", "しょ"),
    ("cha", "ちゃ"),
    ("chu", "ちゅ"),
    ("che", "ちぇ"),
    ("cho", "ちょ"),
    ("ja", "じゃ"),
    ("ju", "じゅ"),
    ("je", "じぇ"),
    ("jo", "じょ"),
    ("tsa", "つぁ"),
    ("tsi", "つぃ"),
    ("tse", "つぇ"),
    ("tso", "つぉ"),
    ("tha", "てゃ"),
    ("thi", "てぃ"),
    ("thu", "てゅ"),
    ("the", "てぇ"),
    ("tho", "てょ"),
    ("dha", "でゃ"),
    ("dhi", "でぃ"),
    ("dhu", "でゅ"),
    ("dhe", "でぇ"),
    ("dho", "でょ"),
    ("twu", "とぅ"),
    ("dwu", "どぅ"),
    ("wha", "うぁ"),
    ("whi", "うぃ"),
    ("whu", "う"),
    ("whe", "うぇ"),
    ("who", "うぉ"),
    ("xa", "ぁ"),
    ("xi", "ぃ"),
    ("xu", "ぅ"),
    ("xe", "ぇ"),
    ("xo", "ぉ"),
    ("la", "ぁ"),
    ("li", "ぃ"),
    ("lu", "ぅ"),
    ("le", "ぇ"),
    ("lo", "ぉ"),
    ("xya", "ゃ"),
    ("xyu", "ゅ"),
    ("xyo", "ょ"),
    ("lya", "ゃ"),
    ("lyu", "ゅ"),
    ("lyo", "ょ"),
    ("xtu", "っ"),
    ("ltu", "っ"),
    ("xtsu", "っ"),
    ("ltsu", "っ"),
    ("xwa", "ゎ"),
    ("lwa", "ゎ"),
    ("kwa", "くぁ"),
    ("gwa", "ぐぁ"),
    ("xn", "ん"),
];

/// Symbols as the composer types them (「」 via [ ] and ・ via /).
const SYMBOL_TABLE: [(char, &str); 31] = [
    (',', "、"),
    ('.', "。"),
    ('?', "？"),
    ('!', "！"),
    ('-', "ー"),
    ('/', "・"),
    ('[', "「"),
    (']', "」"),
    ('~', "〜"),
    ('_', "＿"),
    ('(', "（"),
    (')', "）"),
    (':', "："),
    (';', "；"),
    ('"', "”"),
    ('@', "＠"),
    ('#', "＃"),
    ('$', "＄"),
    ('%', "％"),
    ('&', "＆"),
    ('*', "＊"),
    ('+', "＋"),
    ('=', "＝"),
    ('<', "＜"),
    ('>', "＞"),
    ('{', "｛"),
    ('}', "｝"),
    ('|', "｜"),
    ('^', "＾"),
    ('`', "｀"),
    ('\\', "＼"),
];

const CONTRACTED_ROWS: [(&str, &str); 15] = [
    ("k", "き"),
    ("g", "ぎ"),
    ("s", "し"),
    ("z", "じ"),
    ("t", "ち"),
    ("d", "ぢ"),
    ("n", "に"),
    ("h", "ひ"),
    ("b", "び"),
    ("p", "ぴ"),
    ("m", "み"),
    ("r", "り"),
    ("c", "ち"),
    ("j", "じ"),
    ("f", "ふ"),
];
const CONTRACTED_VOWELS: [(&str, &str); 5] = [
    ("ya", "ゃ"),
    ("yi", "ぃ"),
    ("yu", "ゅ"),
    ("ye", "ぇ"),
    ("yo", "ょ"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes() {
        for (romaji, expected) in [
            ("konnichiha", "こんにちは"),
            ("arigatou", "ありがとう"),
            ("kitte", "きって"),
            ("ra-men", "らーめん"),
            ("shinbun", "しんぶん"),
            ("kan'i", "かんい"),
            ("tenki", "てんき"),
            ("nihongo", "にほんご"),
            ("kyou", "きょう"),
            ("gakkou", "がっこう"),
            ("matcha", "まっちゃ"),
            ("shashin", "しゃしん"),
            ("tsukue", "つくえ"),
            ("fujisan", "ふじさん"),
            ("jouzu", "じょうず"),
            ("onna", "おんな"),
            ("konnnichiha", "こんにちは"),
            ("xtu", "っ"),
            ("dhisuku", "でぃすく"),
            ("wo", "を"),
            ("benkyou", "べんきょう"),
            ("hon", "ほん"),
            ("sushi", "すし"),
        ] {
            let r = RomajiComposer::compose(romaji, false);
            assert_eq!(r.text, expected, "{romaji}");
            assert!(r.is_clean(), "{romaji}");
        }
    }

    /// Delete rewrites the keys from these spellings, so each must compose back to its kana,
    /// also with a vowel typed after it (ん before あ).
    #[test]
    fn respellings_compose_back() {
        for (kana, romaji) in SPELLING.iter() {
            let r = RomajiComposer::compose(romaji, true);
            assert_eq!(r.text, kana.to_string(), "{kana} {romaji}");
            let r = RomajiComposer::compose(&format!("{romaji}a"), false);
            assert_eq!(r.text, format!("{kana}あ"), "{kana} {romaji}a");
        }
    }

    #[test]
    fn pending_while_typing() {
        let r = RomajiComposer::compose("konnic", true);
        assert_eq!(r.text, "こんにc");
        assert_eq!(r.pending, "c");
        assert_eq!(r.stray_letters, 0);
        assert_eq!(RomajiComposer::compose("hon", true).text, "ほn");
    }

    #[test]
    fn non_japanese_leaves_stray_letters() {
        for word in ["strength", "dkssud", "world", "keyboard", "gksrmf"] {
            assert!(
                RomajiComposer::compose(word, false).stray_letters > 0,
                "{word}"
            );
        }
    }

    /// Like real Japanese IMEs, some English converts cleanly (hello → へっぉ);
    /// telling these apart is the detector's job, not the composer's.
    #[test]
    fn some_english_converts_cleanly() {
        assert_eq!(RomajiComposer::compose("hello", false).text, "へっぉ");
    }

    #[test]
    fn katakana() {
        assert_eq!(RomajiComposer::katakana("こんぴゅーたー"), "コンピューター");
    }
}
