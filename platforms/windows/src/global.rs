//! The Han/Eng key as a shortcut, everywhere. The text service only sees keys typed into a
//! text field of an app that loaded it, but a shortcut (Win+D) must work on the desktop, the
//! taskbar and in apps started before the install too. So `rundll32 typescript_tip.dll,KeyHook`
//! (started at sign-in) keeps a low-level keyboard hook: while the Han/Eng key is set to send a
//! shortcut, it takes that key away from everyone and sends the shortcut instead.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::SystemTime;

use typescript_core::store::keys::{HangulAction, KeyAction, Mods};
use typescript_core::Settings;
use windows::core::w;
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, HC_ACTION, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, LLKHF_UP, MSG, WH_KEYBOARD_LL,
};

use crate::keymap::{chord_key, send_shortcut, SENT_BY_US};

#[derive(Default)]
struct State {
    settings: Settings,
    stamp: Option<SystemTime>,
    /// The Han/Eng key while it is held (taken from everyone), and whether another key went
    /// down meanwhile (a modifier used as the Han/Eng key then sends nothing on release).
    held: Option<(u32, bool)>,
}

/// What to do with a key event: take it from everyone, and whether the shortcut goes out now.
#[derive(Debug, PartialEq)]
struct Verdict {
    take: bool,
    send: bool,
}

const PASS: Verdict = Verdict {
    take: false,
    send: false,
};

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

impl State {
    /// Picks up settings.json when the dashboard saved it.
    fn refresh(&mut self) {
        let path = Settings::store_path();
        let stamp = path
            .as_deref()
            .and_then(|p| std::fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());
        if stamp != self.stamp {
            self.stamp = stamp;
            self.settings = Settings::load(path.as_deref());
        }
    }

    fn on_key(&mut self, vk: u32, lparam: isize, up: bool, mods: Mods) -> Verdict {
        if let Some((held, joined)) = self.held {
            if vk != held {
                self.held = Some((held, joined || !up));
                return PASS;
            }
            let mut send = false;
            if up {
                self.held = None;
                let modifier = chord_key(vk, lparam).is_some_and(|k| k.is_modifier());
                send = modifier && !joined;
            }
            return Verdict { take: true, send }; // its auto-repeat and release too
        }
        let Some(key) = chord_key(vk, lparam).filter(|_| !up) else {
            return PASS;
        };
        if self.settings.key_action(mods, key) != Some(KeyAction::Send) {
            return PASS;
        }
        self.held = Some((vk, false));
        // A modifier acts on release, and only if used alone.
        Verdict {
            take: true,
            send: !key.is_modifier(),
        }
    }
}

/// The modifiers held, as the system has them now (a low-level hook runs before the key state
/// of any thread is updated).
fn held_mods() -> Mods {
    // SAFETY: plain Win32 queries.
    let down = |vk: i32| unsafe { GetAsyncKeyState(vk) } < 0;
    Mods {
        ctrl: down(0x11),
        alt: down(0x12),
        shift: down(0x10),
    }
}

fn handle(event: &KBDLLHOOKSTRUCT) -> bool {
    // As a key message's lparam has them: the scan code, and the extended bit.
    let extended = u32::from(event.flags.0 & LLKHF_EXTENDED.0 != 0);
    let lparam = (((event.scanCode & 0xFF) << 16) | (extended << 24)) as isize;
    let up = event.flags.0 & LLKHF_UP.0 != 0;
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !up && state.held.is_none() {
            state.refresh();
        }
        let verdict = state.on_key(event.vkCode, lparam, up, held_mods());
        if verdict.send {
            if let HangulAction::Send(shortcut) = state.settings.hangul_action {
                send_shortcut(&shortcut);
            }
        }
        verdict.take
    })
}

unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to the event's KBDLLHOOKSTRUCT.
        let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        // Keys `send_shortcut` pressed go through untouched.
        if event.dwExtraInfo != SENT_BY_US
            && catch_unwind(AssertUnwindSafe(|| handle(event))).unwrap_or(false)
        {
            return LRESULT(1);
        }
    }
    // SAFETY: passes the event on as received.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Runs the hook until the process ends (one per session: a second one returns at once).
pub fn run() {
    // SAFETY: plain Win32 calls; the mutex lives as long as the process.
    unsafe {
        let _single = CreateMutexW(None, true, w!("Local\\TypescriptKeyHook"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        let module = HINSTANCE(crate::module_handle().0);
        if SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), Some(module), 0).is_err() {
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            DispatchMessageW(&message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXT: isize = 1 << 24;

    fn state(hangul: &str, action: &str) -> State {
        let settings = Settings {
            hangul_key: typescript_core::store::keys::parse_hangul(hangul).unwrap(),
            hangul_action: action.parse().unwrap(),
            ..Settings::default()
        };
        State {
            settings,
            ..State::default()
        }
    }

    fn key(s: &mut State, vk: u32, lparam: isize, up: bool) -> (bool, bool) {
        let v = s.on_key(vk, lparam, up, Mods::NONE);
        (v.take, v.send)
    }

    #[test]
    fn a_lone_right_alt_sends_on_release() {
        let mut s = state("RAlt", "Win+D");
        assert_eq!(key(&mut s, 0xA5, EXT, false), (true, false));
        assert_eq!(key(&mut s, 0xA5, EXT, false), (true, false)); // auto-repeat
        assert_eq!(key(&mut s, 0xA5, EXT, true), (true, true));
        assert_eq!(key(&mut s, 0x41, 0, false), (false, false)); // other keys pass
        assert_eq!(key(&mut s, 0xA4, 0, false), (false, false)); // the left Alt passes
    }

    #[test]
    fn a_right_alt_joined_by_another_key_sends_nothing() {
        let mut s = state("RAlt", "Win+D");
        key(&mut s, 0xA5, EXT, false);
        assert_eq!(key(&mut s, 0x41, 0, false), (false, false));
        assert_eq!(key(&mut s, 0xA5, EXT, true), (true, false));
    }

    #[test]
    fn a_plain_key_sends_once_on_press() {
        let mut s = state("F13", "Win+D");
        assert_eq!(key(&mut s, 0x7C, 0, false), (true, true));
        assert_eq!(key(&mut s, 0x7C, 0, false), (true, false)); // auto-repeat
        assert_eq!(key(&mut s, 0x7C, 0, true), (true, false));
    }

    #[test]
    fn nothing_is_taken_unless_the_key_sends_a_shortcut() {
        let mut s = state("RAlt", "flip");
        assert_eq!(key(&mut s, 0xA5, EXT, false), (false, false));
        assert_eq!(key(&mut s, 0xA5, EXT, true), (false, false));
    }
}
