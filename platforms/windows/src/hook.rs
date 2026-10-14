//! The right Alt as the Han/Eng key. A laptop without a Korean keyboard type sends its Han/Eng
//! key as the right Alt, and TSF never hands Alt to a text service. So, as KeyZen does, a
//! keyboard hook takes it, but only on this thread (no system-wide hook, no extra process):
//! the app never sees the key, and its release does what the Han/Eng key does.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, WH_KEYBOARD,
};

use crate::service::Inner;

thread_local! {
    static HOOK: RefCell<Option<(HHOOK, Weak<Inner>)>> = const { RefCell::new(None) };
}

/// Hooks this thread's keyboard messages for `inner` (once per thread).
pub fn install(inner: &Rc<Inner>) {
    HOOK.with(|slot| {
        if let Some((_, owner)) = slot.borrow_mut().as_mut() {
            *owner = Rc::downgrade(inner); // activated again on this thread
            return;
        }
        // SAFETY: a thread hook with a procedure of this module; removed in `remove`.
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD, Some(proc), None, GetCurrentThreadId()) };
        match hook {
            Ok(hook) => *slot.borrow_mut() = Some((hook, Rc::downgrade(inner))),
            Err(_) => inner.trace("keyboard hook failed"),
        }
    });
}

pub fn remove() {
    if let Some((hook, _)) = HOOK.with(|slot| slot.borrow_mut().take()) {
        // SAFETY: the hook this thread installed.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }
}

/// Whether a keyboard message is the right Alt: VK_RMENU, or VK_MENU marked extended.
fn is_right_alt(vk: usize, lparam: isize) -> bool {
    vk == 0xA5 || (vk == 0x12 && (lparam >> 24) & 1 == 1)
}

unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // HC_ACTION: the message is being taken off the queue (not just peeked at).
    if code == HC_ACTION as i32 && is_right_alt(wparam.0, lparam.0) {
        let inner = HOOK.with(|slot| slot.borrow().as_ref().and_then(|(_, w)| w.upgrade()));
        let up = (lparam.0 >> 31) & 1 == 1;
        if inner.is_some_and(|inner| inner.right_alt(up)) {
            return LRESULT(1);
        }
    }
    // SAFETY: passes the message on as received.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_right_alt_is_taken() {
        let extended = 1 << 24;
        assert!(super::is_right_alt(0xA5, 0));
        assert!(super::is_right_alt(0x12, extended));
        assert!(!super::is_right_alt(0x12, 0)); // the left Alt
        assert!(!super::is_right_alt(0xA3, extended)); // the right Ctrl
    }
}
