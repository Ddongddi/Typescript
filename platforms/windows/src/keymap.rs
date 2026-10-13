//! Virtual keys → the session's `Key`. Assumes a US QWERTY keyboard, like the original.

use typescript_core::Key;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

pub const VK_HANGUL: u32 = 0x15;
const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_MENU: i32 = 0x12;
const VK_CAPITAL: i32 = 0x14;

fn is_down(vk: i32) -> bool {
    // SAFETY: plain Win32 query.
    unsafe { GetKeyState(vk) < 0 }
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
        _ => match printable(vk, shift) {
            Some(c) => Key::Printable(c),
            None => Key::Other,
        },
    })
}
