//! Keys the user assigns to actions: `Esc`, `Alt+Hangul`, `RCtrl`. The syntax, the rules that
//! keep a key from breaking typing, conflicts between keys, and matching a pressed key.

use std::fmt;
use std::str::FromStr;

/// At most this many keys per action.
pub const MAX_KEYS: usize = 8;

/// What an assigned key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Switch the word being typed (or just typed) to its next language.
    Flip,
    /// Pause or resume Typescript in this text field.
    Pause,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
    };

    pub fn count(self) -> usize {
        usize::from(self.ctrl) + usize::from(self.alt) + usize::from(self.shift)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordKey {
    Esc,
    Hangul,
    Hanja,
    CapsLock,
    Pause,
    ScrollLock,
    Space,
    /// F1..=F24.
    F(u8),
    /// 'A'..='Z'.
    Letter(char),
    /// 0..=9.
    Digit(u8),
    /// Modifier keys: `Ctrl` is either side, `RCtrl` only the right one.
    Ctrl,
    Alt,
    Shift,
    RCtrl,
    RAlt,
    RShift,
}

impl ChordKey {
    pub fn is_modifier(self) -> bool {
        matches!(
            self,
            Self::Ctrl | Self::Alt | Self::Shift | Self::RCtrl | Self::RAlt | Self::RShift
        )
    }

    /// Whether an assigned key `self` is satisfied by the key `pressed` (a plain Ctrl takes
    /// either side).
    pub fn accepts(self, pressed: ChordKey) -> bool {
        match self {
            Self::Ctrl => matches!(pressed, Self::Ctrl | Self::RCtrl),
            Self::Alt => matches!(pressed, Self::Alt | Self::RAlt),
            Self::Shift => matches!(pressed, Self::Shift | Self::RShift),
            other => other == pressed,
        }
    }

    fn overlaps(self, other: ChordKey) -> bool {
        self.accepts(other) || other.accepts(self)
    }

    fn parse(token: &str) -> Option<ChordKey> {
        let lower = token.to_ascii_lowercase();
        Some(match lower.as_str() {
            "esc" | "escape" => Self::Esc,
            "hangul" => Self::Hangul,
            "hanja" => Self::Hanja,
            "capslock" => Self::CapsLock,
            "pause" => Self::Pause,
            "scrolllock" => Self::ScrollLock,
            "space" => Self::Space,
            "ctrl" | "control" => Self::Ctrl,
            "alt" => Self::Alt,
            "shift" => Self::Shift,
            "rctrl" => Self::RCtrl,
            "ralt" => Self::RAlt,
            "rshift" => Self::RShift,
            _ => {
                let mut chars = lower.chars();
                let first = chars.next()?;
                if first == 'f' && lower.len() > 1 {
                    let n: u8 = lower[1..].parse().ok()?;
                    return (1..=24).contains(&n).then_some(Self::F(n));
                }
                if chars.next().is_some() {
                    return None;
                }
                match first {
                    'a'..='z' => Self::Letter(first.to_ascii_uppercase()),
                    '0'..='9' => Self::Digit(first as u8 - b'0'),
                    _ => return None,
                }
            }
        })
    }
}

impl fmt::Display for ChordKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Esc => f.write_str("Esc"),
            Self::Hangul => f.write_str("Hangul"),
            Self::Hanja => f.write_str("Hanja"),
            Self::CapsLock => f.write_str("CapsLock"),
            Self::Pause => f.write_str("Pause"),
            Self::ScrollLock => f.write_str("ScrollLock"),
            Self::Space => f.write_str("Space"),
            Self::F(n) => write!(f, "F{n}"),
            Self::Letter(c) => write!(f, "{c}"),
            Self::Digit(d) => write!(f, "{d}"),
            Self::Ctrl => f.write_str("Ctrl"),
            Self::Alt => f.write_str("Alt"),
            Self::Shift => f.write_str("Shift"),
            Self::RCtrl => f.write_str("RCtrl"),
            Self::RAlt => f.write_str("RAlt"),
            Self::RShift => f.write_str("RShift"),
        }
    }
}

/// A key with the modifiers that have to be held with it (none for a modifier key alone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub mods: Mods,
    pub key: ChordKey,
}

impl Chord {
    /// Whether pressing `pressed` with exactly `mods` held is this chord.
    pub fn matches(&self, mods: Mods, pressed: ChordKey) -> bool {
        self.mods == mods && self.key.accepts(pressed)
    }

    /// Whether some key press would be both chords.
    pub fn overlaps(&self, other: &Chord) -> bool {
        self.mods == other.mods && self.key.overlaps(other.key)
    }

    /// Rejects what would break typing or what the system or the program already owns.
    fn check(&self) -> Result<(), String> {
        let Mods { ctrl, alt, shift } = self.mods;
        let typed = matches!(
            self.key,
            ChordKey::Letter(_) | ChordKey::Digit(_) | ChordKey::Space
        );
        if typed && !ctrl && !alt {
            return Err(format!("{self} would type a character: add Ctrl or Alt"));
        }
        if let ChordKey::Letter(c) = self.key {
            if ctrl && !alt && !shift && "CVXZYASJK".contains(c) {
                return Err(format!("{self} is a shortcut other programs use"));
            }
        }
        let reserved = match self.key {
            ChordKey::Esc => ctrl || alt || shift,
            ChordKey::Space => alt && !ctrl,
            ChordKey::F(4) => alt,
            _ => false,
        };
        if reserved {
            return Err(format!("{self} is already used by Windows or Typescript"));
        }
        Ok(())
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (held, name) in [
            (self.mods.ctrl, "Ctrl"),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
        ] {
            if held {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

fn is_windows_key(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "win" | "windows" | "meta" | "cmd" | "super"
    )
}

impl FromStr for Chord {
    type Err = String;

    fn from_str(text: &str) -> Result<Chord, String> {
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let Some((last, held)) = parts.split_last() else {
            return Err("empty key".into());
        };
        let mut mods = Mods::NONE;
        for part in held {
            if is_windows_key(part) {
                return Err("the Windows key can't be used: Windows handles it first".into());
            }
            let flag = match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut mods.ctrl,
                "alt" => &mut mods.alt,
                "shift" => &mut mods.shift,
                _ => return Err(format!("'{part}' is not a modifier (Ctrl, Alt, Shift)")),
            };
            if *flag {
                return Err(format!("'{part}' is given twice"));
            }
            *flag = true;
        }
        if is_windows_key(last) {
            return Err("the Windows key can't be used: Windows handles it first".into());
        }
        let key = ChordKey::parse(last)
            .ok_or_else(|| format!("'{last}' is not a key that can be assigned"))?;
        if key.is_modifier() && mods.count() > 0 {
            return Err("a modifier key alone can't be combined with other modifiers".into());
        }
        let chord = Chord { mods, key };
        chord.check()?;
        Ok(chord)
    }
}

fn builtin(text: &str) -> Chord {
    text.parse().expect("built-in key is valid")
}

pub fn default_flip() -> Vec<Chord> {
    vec![builtin("Esc"), builtin("Hangul")]
}

pub fn default_pause() -> Vec<Chord> {
    vec![builtin("Alt+Hangul")]
}

fn parse_list(name: &str, texts: &[String]) -> Result<Vec<Chord>, String> {
    if texts.len() > MAX_KEYS {
        return Err(format!("{name} takes at most {MAX_KEYS} keys"));
    }
    let mut out: Vec<Chord> = Vec::new();
    for text in texts {
        let chord: Chord = text.parse().map_err(|e| format!("{name}: {e}"))?;
        if let Some(same) = out.iter().find(|c| c.overlaps(&chord)) {
            return Err(format!("{name}: {chord} is the same key as {same}"));
        }
        out.push(chord);
    }
    Ok(out)
}

/// Both lists as the dashboard sends them: every mistake comes back as a message.
pub fn validate(flip: &[String], pause: &[String]) -> Result<(Vec<Chord>, Vec<Chord>), String> {
    let flip = parse_list("flipKeys", flip)?;
    let pause = parse_list("pauseKeys", pause)?;
    for chord in &pause {
        if let Some(other) = flip.iter().find(|f| f.overlaps(chord)) {
            return Err(format!(
                "{chord} is already a language switch key ({other})"
            ));
        }
    }
    Ok((flip, pause))
}

/// A list from a settings.json somebody edited by hand: unusable entries are dropped, and
/// keys in `taken` (the other action's) win.
pub fn sanitize(texts: &[&str], taken: &[Chord]) -> Vec<Chord> {
    let mut out: Vec<Chord> = Vec::new();
    for text in texts {
        let Ok(chord) = text.parse::<Chord>() else {
            continue;
        };
        if out.len() < MAX_KEYS && !out.iter().chain(taken).any(|c| c.overlaps(&chord)) {
            out.push(chord);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        text.parse().unwrap()
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn names_round_trip_in_one_spelling() {
        for (given, shown) in [
            ("alt+hangul", "Alt+Hangul"),
            ("ESC", "Esc"),
            ("shift + ctrl + a", "Ctrl+Shift+A"),
            ("rctrl", "RCtrl"),
            ("f13", "F13"),
            ("Ctrl+Space", "Ctrl+Space"),
            ("Alt+7", "Alt+7"),
        ] {
            assert_eq!(chord(given).to_string(), shown, "{given}");
        }
    }

    #[test]
    fn keys_that_break_typing_or_belong_to_others_are_refused() {
        for bad in [
            "Win+L",
            "Windows",
            "L",
            "7",
            "Space",
            "Shift+Space",
            "Shift+A",
            "Ctrl+C",
            "Ctrl+V",
            "Ctrl+J",
            "Ctrl+Esc",
            "Alt+Esc",
            "Shift+Esc",
            "Alt+F4",
            "Alt+Space",
            "Enter",
            "Tab",
            "Ctrl+Shift",
            "Ctrl+Ctrl",
            "F25",
            "F0",
            "",
            "Ctrl+",
        ] {
            assert!(bad.parse::<Chord>().is_err(), "{bad} should be refused");
        }
        for good in [
            "Esc",
            "Hangul",
            "Ctrl",
            "RAlt",
            "Ctrl+Space",
            "Ctrl+Shift+C",
            "Alt+Hangul",
            "F13",
            "CapsLock",
            "Ctrl+Alt+L",
        ] {
            assert!(good.parse::<Chord>().is_ok(), "{good} should be taken");
        }
    }

    #[test]
    fn exact_modifiers() {
        let plain = chord("Hangul");
        let with_alt = chord("Alt+Hangul");
        let alt = Mods {
            alt: true,
            ..Mods::NONE
        };
        assert!(plain.matches(Mods::NONE, ChordKey::Hangul));
        assert!(!plain.matches(alt, ChordKey::Hangul));
        assert!(with_alt.matches(alt, ChordKey::Hangul));
        assert!(!with_alt.matches(Mods::NONE, ChordKey::Hangul));
    }

    #[test]
    fn a_plain_modifier_takes_either_side_a_right_one_only_right() {
        assert!(chord("Ctrl").matches(Mods::NONE, ChordKey::RCtrl));
        assert!(chord("Ctrl").matches(Mods::NONE, ChordKey::Ctrl));
        assert!(chord("RCtrl").matches(Mods::NONE, ChordKey::RCtrl));
        assert!(!chord("RCtrl").matches(Mods::NONE, ChordKey::Ctrl));
    }

    #[test]
    fn the_defaults_are_valid_and_do_not_clash() {
        let flip: Vec<String> = default_flip().iter().map(|c| c.to_string()).collect();
        let pause: Vec<String> = default_pause().iter().map(|c| c.to_string()).collect();
        assert_eq!(flip, ["Esc", "Hangul"]);
        assert_eq!(pause, ["Alt+Hangul"]);
        assert!(validate(&flip, &pause).is_ok());
    }

    #[test]
    fn conflicts_are_refused_with_a_reason() {
        let across = validate(&strings(&["Esc", "Hangul"]), &strings(&["Hangul"]));
        assert!(across
            .unwrap_err()
            .contains("already a language switch key"));
        let twice = validate(&strings(&["Esc", "esc"]), &[]);
        assert!(twice.unwrap_err().contains("same key"));
        let sides = validate(&strings(&["Ctrl"]), &strings(&["RCtrl"]));
        assert!(sides.is_err());
        let fine = validate(&strings(&["Ctrl"]), &strings(&["RAlt"]));
        assert!(fine.is_ok());
        let nine: Vec<String> = (1..=9).map(|n| format!("F{n}")).collect();
        assert!(validate(&nine, &[]).unwrap_err().contains("at most"));
        assert!(validate(&[], &[]).is_ok(), "no keys at all is allowed");
    }

    #[test]
    fn hand_edited_lists_keep_what_is_usable() {
        let taken = [chord("Hangul")];
        let kept = sanitize(
            &["Win+L", "F13", "f13", "Hangul", "Ctrl+Space", "nope"],
            &taken,
        );
        let shown: Vec<String> = kept.iter().map(|c| c.to_string()).collect();
        assert_eq!(shown, ["F13", "Ctrl+Space"]);
    }
}
