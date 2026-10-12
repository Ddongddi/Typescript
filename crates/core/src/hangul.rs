//! Hangul jamo tables, the 2-set (두벌식) QWERTY layout and the composing automaton.
//! Port of Jamo.swift + HangulComposer.swift. Jamo are Hangul Compatibility Jamo (U+3131…).

const CHOSEONG: &str = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ";
const JUNGSEONG: &str = "ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ";
/// Index 0 (no final) is handled separately.
const JONGSEONG: &str = "ㄱㄲㄳㄴㄵㄶㄷㄹㄺㄻㄼㄽㄾㄿㅀㅁㅂㅄㅅㅆㅇㅈㅊㅋㅌㅍㅎ";

pub(crate) fn cho_index(c: char) -> Option<usize> {
    CHOSEONG.chars().position(|x| x == c)
}
pub(crate) fn jung_index(c: char) -> Option<usize> {
    JUNGSEONG.chars().position(|x| x == c)
}
/// 1-based (0 means "no final consonant").
pub(crate) fn jong_index(c: char) -> Option<usize> {
    JONGSEONG.chars().position(|x| x == c).map(|i| i + 1)
}

fn vowel_combo(a: char, b: char) -> Option<char> {
    Some(match (a, b) {
        ('ㅗ', 'ㅏ') => 'ㅘ',
        ('ㅗ', 'ㅐ') => 'ㅙ',
        ('ㅗ', 'ㅣ') => 'ㅚ',
        ('ㅜ', 'ㅓ') => 'ㅝ',
        ('ㅜ', 'ㅔ') => 'ㅞ',
        ('ㅜ', 'ㅣ') => 'ㅟ',
        ('ㅡ', 'ㅣ') => 'ㅢ',
        _ => return None,
    })
}

const FINAL_COMBOS: [(char, char, char); 11] = [
    ('ㄱ', 'ㅅ', 'ㄳ'),
    ('ㄴ', 'ㅈ', 'ㄵ'),
    ('ㄴ', 'ㅎ', 'ㄶ'),
    ('ㄹ', 'ㄱ', 'ㄺ'),
    ('ㄹ', 'ㅁ', 'ㄻ'),
    ('ㄹ', 'ㅂ', 'ㄼ'),
    ('ㄹ', 'ㅅ', 'ㄽ'),
    ('ㄹ', 'ㅌ', 'ㄾ'),
    ('ㄹ', 'ㅍ', 'ㄿ'),
    ('ㄹ', 'ㅎ', 'ㅀ'),
    ('ㅂ', 'ㅅ', 'ㅄ'),
];

fn final_combo(a: char, b: char) -> Option<char> {
    FINAL_COMBOS.iter().find(|(x, y, _)| *x == a && *y == b).map(|t| t.2)
}

/// Compound final → (stays as final, moves to the next syllable).
fn final_split(c: char) -> Option<(char, char)> {
    FINAL_COMBOS.iter().find(|t| t.2 == c).map(|t| (t.0, t.1))
}

pub fn is_consonant(j: char) -> bool {
    cho_index(j).is_some() || jong_index(j).is_some()
}
pub fn is_vowel(j: char) -> bool {
    jung_index(j).is_some()
}

fn syllable(cho: char, jung: char, jong: Option<char>) -> char {
    let c = cho_index(cho).unwrap();
    let v = jung_index(jung).unwrap();
    let f = jong.and_then(jong_index).unwrap_or(0);
    char::from_u32(0xAC00 + ((c * 21 + v) * 28 + f) as u32).unwrap()
}

pub struct DubeolsikLayout;

impl DubeolsikLayout {
    fn lower(c: char) -> Option<char> {
        Some(match c {
            'q' => 'ㅂ', 'w' => 'ㅈ', 'e' => 'ㄷ', 'r' => 'ㄱ', 't' => 'ㅅ', 'y' => 'ㅛ', 'u' => 'ㅕ',
            'i' => 'ㅑ', 'o' => 'ㅐ', 'p' => 'ㅔ', 'a' => 'ㅁ', 's' => 'ㄴ', 'd' => 'ㅇ', 'f' => 'ㄹ',
            'g' => 'ㅎ', 'h' => 'ㅗ', 'j' => 'ㅓ', 'k' => 'ㅏ', 'l' => 'ㅣ', 'z' => 'ㅋ', 'x' => 'ㅌ',
            'c' => 'ㅊ', 'v' => 'ㅍ', 'b' => 'ㅠ', 'n' => 'ㅜ', 'm' => 'ㅡ',
            _ => return None,
        })
    }

    fn shifted(c: char) -> Option<char> {
        Some(match c {
            'Q' => 'ㅃ', 'W' => 'ㅉ', 'E' => 'ㄸ', 'R' => 'ㄲ', 'T' => 'ㅆ', 'O' => 'ㅒ', 'P' => 'ㅖ',
            _ => return None,
        })
    }

    /// Keys whose shifted form produces a different jamo. Shift on any other letter
    /// is a signal the user is typing English.
    pub fn is_korean_shift_key(key: char) -> bool {
        Self::shifted(key).is_some()
    }

    /// The jamo a key produces, or None for non-letters. Uppercase letters without a
    /// shifted jamo fall back to the lowercase jamo (as the macOS 2-set keyboard does).
    pub fn jamo(key: char) -> Option<char> {
        if let Some(j) = Self::shifted(key) {
            return Some(j);
        }
        if !key.is_ascii() {
            return None;
        }
        Self::lower(key.to_ascii_lowercase())
    }

    /// A Korean syllable has to start with a consonant key.
    pub fn starts_syllable(key: char) -> bool {
        Self::jamo(key).map(is_consonant).unwrap_or(false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Syllable(char),
    /// A jamo that never became part of a complete syllable.
    Jamo(char),
}

impl Unit {
    pub fn character(self) -> char {
        match self {
            Unit::Syllable(c) | Unit::Jamo(c) => c,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composed {
    pub units: Vec<Unit>,
}

impl Composed {
    pub fn text(&self) -> String {
        self.units.iter().map(|u| u.character()).collect()
    }
    pub fn syllables(&self) -> Vec<char> {
        self.units.iter().filter_map(|u| if let Unit::Syllable(c) = u { Some(*c) } else { None }).collect()
    }
    pub fn stray_jamo(&self) -> Vec<char> {
        self.units.iter().filter_map(|u| if let Unit::Jamo(c) = u { Some(*c) } else { None }).collect()
    }
    /// True when the last unit is a lone consonant: the user just started the next syllable.
    pub fn ends_with_pending_consonant(&self) -> bool {
        matches!(self.units.last(), Some(Unit::Jamo(c)) if is_consonant(*c))
    }
    /// Stray jamo other than a consonant just typed to start the next syllable.
    pub fn is_broken(&self) -> bool {
        self.stray_jamo().len() > usize::from(self.ends_with_pending_consonant())
    }
}

pub struct HangulComposer;

impl HangulComposer {
    /// None if any key is not a letter on the 2-set layout.
    pub fn compose(keys: &str) -> Option<Composed> {
        let jamos: Option<Vec<char>> = keys.chars().map(DubeolsikLayout::jamo).collect();
        jamos.map(|j| Self::compose_jamos(&j))
    }

    pub fn compose_jamos(jamos: &[char]) -> Composed {
        let mut units = Vec::new();
        let (mut cho, mut jung, mut jong): (Option<char>, Option<char>, Option<char>) = (None, None, None);

        fn flush(units: &mut Vec<Unit>, cho: &mut Option<char>, jung: &mut Option<char>, jong: &mut Option<char>) {
            match (*cho, *jung) {
                (Some(c), Some(v)) => units.push(Unit::Syllable(syllable(c, v, *jong))),
                _ => {
                    if let Some(c) = *cho {
                        units.push(Unit::Jamo(c));
                    }
                    if let Some(v) = *jung {
                        units.push(Unit::Jamo(v));
                    }
                }
            }
            *cho = None;
            *jung = None;
            *jong = None;
        }

        for &j in jamos {
            if is_vowel(j) {
                if let Some(v) = jung {
                    if let Some(fin) = jong {
                        // 받침 moves to the next syllable's initial: 갑+ㅏ → 가바, 값+ㅏ → 갑사
                        if let Some((stay, mv)) = final_split(fin) {
                            jong = Some(stay);
                            flush(&mut units, &mut cho, &mut jung, &mut jong);
                            cho = Some(mv);
                        } else {
                            jong = None;
                            flush(&mut units, &mut cho, &mut jung, &mut jong);
                            cho = Some(fin);
                        }
                        jung = Some(j);
                    } else if let Some(c) = vowel_combo(v, j) {
                        jung = Some(c);
                    } else {
                        flush(&mut units, &mut cho, &mut jung, &mut jong);
                        jung = Some(j);
                    }
                } else {
                    jung = Some(j);
                }
            } else if jung.is_none() || cho.is_none() {
                // Nothing to attach to (or a bare vowel): start a new syllable.
                if cho.is_some() || jung.is_some() {
                    flush(&mut units, &mut cho, &mut jung, &mut jong);
                }
                cho = Some(j);
            } else if let Some(fin) = jong {
                if let Some(c) = final_combo(fin, j) {
                    jong = Some(c);
                } else {
                    flush(&mut units, &mut cho, &mut jung, &mut jong);
                    cho = Some(j);
                }
            } else if jong_index(j).is_some() {
                jong = Some(j);
            } else {
                // ㄸ ㅃ ㅉ cannot be finals.
                flush(&mut units, &mut cho, &mut jung, &mut jong);
                cho = Some(j);
            }
        }
        flush(&mut units, &mut cho, &mut jung, &mut jong);
        Composed { units }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes() {
        for (keys, expected) in [
            ("dkssudgktpdy", "안녕하세요"), ("gksrmf", "한글"), ("rkqt", "값"), ("rkqtdl", "값이"),
            ("dlTek", "있다"), ("dhkswjsgl", "완전히"), ("ehlwl", "되지"), ("dmlwk", "의자"),
            ("ekfrdl", "닭이"), ("rhkswls", "관진"), ("Qkffl", "빨리"), ("dkQk", "아빠"),
            ("zzz", "ㅋㅋㅋ"), ("rk", "가"), ("r", "ㄱ"), ("k", "ㅏ"),
        ] {
            assert_eq!(HangulComposer::compose(keys).unwrap().text(), expected, "{keys}");
        }
    }

    #[test]
    fn double_consonant_that_cannot_be_final_starts_new_syllable() {
        let r = HangulComposer::compose("dkE").unwrap();
        assert_eq!(r.text(), "아ㄸ");
        assert!(r.ends_with_pending_consonant());
    }

    #[test]
    fn english_produces_stray_jamo() {
        assert!(!HangulComposer::compose("hello").unwrap().stray_jamo().is_empty());
    }

    #[test]
    fn rejects_non_letters() {
        assert!(HangulComposer::compose("ab1").is_none());
    }
}
