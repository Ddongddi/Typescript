//! C interface of the input method, for the macOS and Linux text services. Same contract as the
//! Android bridge (`typescript_android::ime`): a key and the text before the caret go in, a
//! string comes out — `1` if the key was consumed (else `0`), then each command preceded by
//! `\u{1}`: `M<text>` set the composing text, `I<text>` commit it (replacing the composing
//! text), `B<n>` delete n UTF-16 units before the caret. Free a returned string with
//! `ts_string_free`. Nothing here unwinds into the caller.

use std::ffi::{c_char, c_int, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Mutex;

use typescript_android::ime::Ime;

/// The instance behind a handle from `ts_new`.
pub struct Handle(Mutex<Ime>);

fn text(s: *const c_char) -> String {
    if s.is_null() {
        return String::new();
    }
    // SAFETY: the caller passes a NUL-terminated string that stays valid for this call.
    unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
}

fn with<R>(h: *mut Handle, fallback: R, f: impl FnOnce(&mut Ime) -> R) -> R {
    if h.is_null() {
        return fallback;
    }
    // SAFETY: `h` comes from `ts_new` and is used until `ts_free`.
    let handle = unsafe { &*h };
    catch_unwind(AssertUnwindSafe(|| {
        f(&mut handle.0.lock().unwrap_or_else(|e| e.into_inner()))
    }))
    .unwrap_or(fallback)
}

fn reply(s: String) -> *mut c_char {
    CString::new(s.replace('\0', "")).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// A new input method whose settings, apps and lexicon live in `dir`. NULL on failure.
#[no_mangle]
pub extern "C" fn ts_new(dir: *const c_char) -> *mut Handle {
    let dir = PathBuf::from(text(dir));
    catch_unwind(|| Box::into_raw(Box::new(Handle(Mutex::new(Ime::new(&dir))))))
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn ts_free(h: *mut Handle) {
    if !h.is_null() {
        // SAFETY: the handle is not used after this call.
        drop(unsafe { Box::from_raw(h) });
    }
}

#[no_mangle]
pub extern "C" fn ts_string_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: `s` came from this library's `reply`.
        drop(unsafe { CString::from_raw(s) });
    }
}

/// A new text field in the program `app` (picks its rule; NULL or "" for unknown).
#[no_mangle]
pub extern "C" fn ts_activate(h: *mut Handle, app: *const c_char) {
    let app = text(app);
    with(h, (), |ime| ime.activate(&app));
}

/// `kind`: 0 letter, 1 printable, 2 delete, 3 escape, 4 cycle, 5 enter, 6 tab, else other.
#[no_mangle]
pub extern "C" fn ts_key(
    h: *mut Handle,
    kind: c_int,
    ch: u32,
    before: *const c_char,
) -> *mut c_char {
    let (ch, before) = (char::from_u32(ch).unwrap_or('\0'), text(before));
    reply(with(h, "0".to_string(), |ime| ime.key(kind, ch, &before)))
}

#[no_mangle]
pub extern "C" fn ts_reset(h: *mut Handle) {
    with(h, (), |ime| ime.reset());
}

#[no_mangle]
pub extern "C" fn ts_toggle_pause(h: *mut Handle, before: *const c_char) -> *mut c_char {
    let before = text(before);
    reply(with(h, "0".to_string(), |ime| ime.toggle_pause(&before)))
}

#[no_mangle]
pub extern "C" fn ts_paused(h: *mut Handle) -> c_int {
    with(h, false, |ime| ime.paused()) as c_int
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(h: *mut Handle, kind: c_int, ch: char, before: &str) -> String {
        let before = CString::new(before).unwrap();
        let p = ts_key(h, kind, ch as u32, before.as_ptr());
        // SAFETY: `p` is a string this library just returned.
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        ts_string_free(p);
        s
    }

    /// Plays the commands onto the text the way a text field would.
    fn apply(field: &mut String, composing: &mut String, out: &str) {
        for op in out.split('\u{1}').skip(1) {
            let (tag, rest) = op.split_at(1);
            match tag {
                "M" | "I" => {
                    field.truncate(field.len() - composing.len());
                    field.push_str(rest);
                    *composing = if tag == "M" {
                        rest.into()
                    } else {
                        String::new()
                    };
                }
                _ => {}
            }
        }
    }

    #[test]
    fn typing_dkssud_gives_korean_through_the_c_interface() {
        let dir = std::env::temp_dir().join(format!("ts-ffi-{}", std::process::id()));
        let dir_c = CString::new(dir.to_str().unwrap()).unwrap();
        let h = ts_new(dir_c.as_ptr());
        assert!(!h.is_null());
        ts_activate(h, c"notepad".as_ptr());
        let (mut field, mut composing) = (String::new(), String::new());
        for ch in "dkssud".chars() {
            let out = call(h, 0, ch, &field);
            assert!(out.starts_with('1'), "{out:?}");
            apply(&mut field, &mut composing, &out);
        }
        let out = call(h, 5, '\0', &field);
        apply(&mut field, &mut composing, &out);
        assert_eq!(field.trim_end(), "안녕");
        assert_eq!(ts_paused(h), 0);
        ts_free(h);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_null_handle_is_ignored() {
        let p = ts_key(std::ptr::null_mut(), 0, 'a' as u32, std::ptr::null());
        // SAFETY: just returned by this library.
        assert_eq!(unsafe { CStr::from_ptr(p) }.to_str().unwrap(), "0");
        ts_string_free(p);
        ts_free(std::ptr::null_mut());
    }
}
