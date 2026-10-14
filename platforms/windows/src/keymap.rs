//! Virtual keys → the session's `Key`. Assumes a US QWERTY keyboard, like the original.

use typescript_core::store::keys::{ChordKey, Mods};
use typescript_core::Key;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

pub const VK_HANGUL: u32 = 0x15;
const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_J: u32 = 0x4A;
const VK_K: u32 = 0x4B;
const VK_F6: u32 = 0x75;
const VK_F7: u32 = 0x76;
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_MENU: i32 = 0x12;
const VK_CAPITAL: i32 = 0x14;

fn is_down(vk: i32) -> bool {
    // SAFETY: plain Win32 query.
    unsafe { GetKeyState(vk) < 0 }
}

/// The modifiers held right now.
pub fn held_mods() -> Mods {
    Mods {
        ctrl: is_down(VK_CONTROL),
        alt: is_down(VK_MENU),
        shift: is_down(VK_SHIFT),
    }
}

/// The assignable key a key event is; `lparam` tells the right Ctrl/Alt/Shift from the left.
pub fn chord_key(vk: u32, lparam: isize) -> Option<ChordKey> {
    let extended = (lparam >> 24) & 1 == 1;
    let scan = (lparam >> 16) & 0xFF;
    Some(match vk {
        VK_ESCAPE => ChordKey::Esc,
        VK_HANGUL => ChordKey::Hangul,
        0x19 => ChordKey::Hanja,
        0x14 => ChordKey::CapsLock,
        0x13 => ChordKey::Pause,
        0x91 => ChordKey::ScrollLock,
        VK_SPACE => ChordKey::Space,
        0x70..=0x87 => ChordKey::F((vk - 0x6F) as u8),
        0x41..=0x5A => ChordKey::Letter(vk as u8 as char),
        0x30..=0x39 => ChordKey::Digit((vk - 0x30) as u8),
        0x10 | 0xA0 | 0xA1 if vk == 0xA1 || (vk == 0x10 && scan == 0x36) => ChordKey::RShift,
        0x10 | 0xA0 | 0xA1 => ChordKey::Shift,
        0x11 | 0xA2 | 0xA3 if vk == 0xA3 || (vk == 0x11 && extended) => ChordKey::RCtrl,
        0x11 | 0xA2 | 0xA3 => ChordKey::Ctrl,
        0x12 | 0xA4 | 0xA5 if vk == 0xA5 || (vk == 0x12 && extended) => ChordKey::RAlt,
        0x12 | 0xA4 | 0xA5 => ChordKey::Alt,
        _ => return None,
    })
}

/// The characters of a US QWERTY keyboard, which is what the original assumes too.
fn printable(vk: u32, shift: bool) -> Option<char> {
    let pick = |a: char, b: char| Some(if shift { b } else { a });
    match vk {
        0x20 => Some(' '),
        0x30 => pick('0', ')'),
        0x31 => pick('1', '!'),
        0x32 => pick('2', '@'),
        0x33 => pick('3', '#'),
        0x34 => pick('4', '$'),
        0x35 => pick('5', '%'),
        0x36 => pick('6', '^'),
        0x37 => pick('7', '&'),
        0x38 => pick('8', '*'),
        0x39 => pick('9', '('),
        0xBA => pick(';', ':'),
        0xBB => pick('=', '+'),
        0xBC => pick(',', '<'),
        0xBD => pick('-', '_'),
        0xBE => pick('.', '>'),
        0xBF => pick('/', '?'),
        0xC0 => pick('`', '~'),
        0xDB => pick('[', '{'),
        0xDC => pick('\\', '|'),
        0xDD => pick(']', '}'),
        0xDE => pick('\'', '"'),
        0x60..=0x69 => Some((b'0' + (vk - 0x60) as u8) as char),
        0x6A => Some('*'),
        0x6B => Some('+'),
        0x6D => Some('-'),
        0x6E => Some('.'),
        0x6F => Some('/'),
        _ => None,
    }
}

/// Keys that mean nothing to a word being typed: modifiers, locks, IME keys.
fn is_ignored(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 | 0xA0
            ..=0xA5 | 0x5B | 0x5C | 0x14 | 0x90 | 0x91 | 0x15 | 0x17 | 0x19 | 0xE5 | 0xFF
    )
}

/// The session's view of a virtual key, or None for keys it must not see at all.
pub fn key_for(vk: u32) -> Option<Key> {
    if is_ignored(vk) {
        return None;
    }
    let (shift, ctrl, alt) = (is_down(VK_SHIFT), is_down(VK_CONTROL), is_down(VK_MENU));
    if ctrl || alt {
        // ⌃J / ⌃K: the Japanese reading as hiragana / katakana, when a phrase is showing
        // (the session commits and passes them on otherwise).
        if ctrl && !alt && !shift {
            match vk {
                VK_J => return Some(Key::Hiragana),
                VK_K => return Some(Key::Katakana),
                _ => {}
            }
        }
        return Some(Key::Other); // a shortcut: the word goes in, the app gets the key
    }
    Some(match vk {
        0x41..=0x5A => {
            let upper = shift != (unsafe { GetKeyState(VK_CAPITAL) } & 1 != 0);
            let c = vk as u8 as char;
            Key::Letter(if upper { c } else { c.to_ascii_lowercase() })
        }
        VK_BACK => Key::Delete,
        VK_TAB if !shift => Key::Tab,
        VK_ESCAPE if shift => Key::Cycle,
        VK_ESCAPE => Key::Escape,
        VK_RETURN => Key::Enter,
        VK_LEFT => Key::Left,
        VK_UP => Key::Up,
        VK_RIGHT => Key::Right,
        VK_DOWN => Key::Down,
        VK_F7 => Key::Katakana,
        VK_F6 => Key::Hiragana,
        VK_SPACE if shift => Key::ShiftSpace,
        _ => match printable(vk, shift) {
            Some(c) => Key::Printable(c),
            None => Key::Other,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_become_assignable_keys() {
        assert_eq!(chord_key(0x15, 0), Some(ChordKey::Hangul));
        assert_eq!(chord_key(0x1B, 0), Some(ChordKey::Esc));
        assert_eq!(chord_key(0x70, 0), Some(ChordKey::F(1)));
        assert_eq!(chord_key(0x41, 0), Some(ChordKey::Letter('A')));
        assert_eq!(chord_key(0x0D, 0), None);
    }

    #[test]
    fn modifiers_tell_left_from_right() {
        assert_eq!(chord_key(0x11, 0), Some(ChordKey::Ctrl));
        assert_eq!(chord_key(0x11, 1 << 24), Some(ChordKey::RCtrl));
        assert_eq!(chord_key(0xA3, 0), Some(ChordKey::RCtrl));
        assert_eq!(chord_key(0x12, 1 << 24), Some(ChordKey::RAlt));
        assert_eq!(chord_key(0x10, 0x36 << 16), Some(ChordKey::RShift));
        assert_eq!(chord_key(0x10, 0x2A << 16), Some(ChordKey::Shift));
    }
}
