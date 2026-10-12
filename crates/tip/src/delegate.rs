//! Hands the process over to the system's Korean IME, for apps where Typescript is set to step
//! aside (`"useSystemIme": true` in apps.json): the user then has the Han/Eng key as in any
//! other app, and Typescript's composition never meets the app's text handling.
//!
//! Switching profiles deactivates this very text service, so it is not done inside TSF's own
//! callback: a message to a hidden window makes it happen right after the callback returns.
//! Failing (no Korean IME installed) just leaves Typescript in place.
//!
//! The active input method is shared between apps, so it would stay the system IME in every
//! other app too. A foreground hook therefore puts back what was active before as soon as
//! another process's window comes to the front.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};

use windows::core::{w, GUID};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::guids::LANGID_KO_KR;

/// Microsoft's Korean IME: its CLSID and profile, as they appear in the user's language list
/// (`0412:{A028AE76-…}{B5FE1F02-…}`).
const SYSTEM_IME_CLSID: GUID = GUID::from_u128(0xA028AE76_01B1_46C2_99C4_ACD9858AE02F);
const SYSTEM_IME_PROFILE: GUID = GUID::from_u128(0xB5FE1F02_D5F2_4445_9C03_C568F23C99A1);
const TF_PROFILETYPE_INPUTPROCESSOR: u32 = 1;

const HAND_OVER: u32 = WM_APP + 1;
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const WINEVENT_OUTOFCONTEXT: u32 = 0;
static REGISTER: Once = Once::new();
static HOOK: Once = Once::new();

/// The text service that was active before the hand-over: (class, profile, language).
static PREVIOUS: Mutex<Option<(GUID, GUID, u16)>> = Mutex::new(None);
/// The system IME is active because of a hand-over (so leaving the app should undo it).
static HANDED_OVER: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == HAND_OVER {
        let _ = std::panic::catch_unwind(switch);
        let _ = DestroyWindow(hwnd);
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

fn profiles() -> Option<ITfInputProcessorProfileMgr> {
    // SAFETY: COM object created and used on this thread.
    unsafe { CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER).ok() }
}

fn activate(
    profiles: &ITfInputProcessorProfileMgr,
    langid: u16,
    clsid: &GUID,
    profile: &GUID,
    flags: u32,
) {
    // SAFETY: plain COM call with live pointers.
    let _ = unsafe {
        profiles.ActivateProfile(
            TF_PROFILETYPE_INPUTPROCESSOR,
            langid,
            clsid,
            profile,
            HKL::default(),
            flags,
        )
    };
}

/// Remembers the active text service and activates the system Korean IME (applied by TSF
/// asynchronously, once the thread's messages are processed).
fn switch_profile() {
    let Some(profiles) = profiles() else { return };
    // Remember what to put back (unless the system IME is already what's active).
    // SAFETY: the struct is plain data, filled by the call.
    unsafe {
        let mut active: TF_INPUTPROCESSORPROFILE = std::mem::zeroed();
        if profiles
            .GetActiveProfile(&GUID_TFCAT_TIP_KEYBOARD, &mut active)
            .is_ok()
            && active.clsid != SYSTEM_IME_CLSID
        {
            *PREVIOUS.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((active.clsid, active.guidProfile, active.langid));
        }
    }
    activate(
        &profiles,
        LANGID_KO_KR,
        &SYSTEM_IME_CLSID,
        &SYSTEM_IME_PROFILE,
        TF_IPPMF_FORPROCESS | TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
    );
}

fn switch() {
    switch_profile();
    HANDED_OVER.store(true, Ordering::SeqCst);
    HOOK.call_once(|| {
        crate::pin_module();
        // SAFETY: out-of-context hook; the callback lives in this (pinned) DLL.
        let _ = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(foreground_changed),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
    });
}

/// Another process's window came to the front: put the previous text service back.
unsafe extern "system" fn foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let _ = std::panic::catch_unwind(|| {
        if hwnd.is_invalid() || !HANDED_OVER.load(Ordering::SeqCst) {
            return;
        }
        let mut pid = 0u32;
        // SAFETY: plain Win32 query.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 || pid == unsafe { GetCurrentProcessId() } {
            return; // still this app (its own dialogs, popups)
        }
        restore();
    });
}

fn restore() {
    let previous = PREVIOUS.lock().unwrap_or_else(|e| e.into_inner()).take();
    let (clsid, profile, langid) = previous.unwrap_or((
        crate::guids::CLSID_TYPESCRIPT,
        crate::guids::GUID_PROFILE,
        crate::guids::LANGID_TYPESCRIPT,
    ));
    HANDED_OVER.store(false, Ordering::SeqCst);
    if let Some(profiles) = profiles() {
        activate(
            &profiles,
            langid,
            &clsid,
            &profile,
            TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
        );
    }
}

/// Asks (from the UI thread's message loop) for this process to switch to the system Korean IME.
pub fn hand_over() {
    // SAFETY: a class and a message-only window owned by this thread; the window destroys itself.
    unsafe {
        let instance = HINSTANCE(crate::module_handle().0);
        REGISTER.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: w!("TypescriptHandOver"),
                ..Default::default()
            };
            RegisterClassW(&class);
        });
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("TypescriptHandOver"),
            None,
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        ) else {
            return;
        };
        let _ = PostMessageW(Some(hwnd), HAND_OVER, WPARAM(0), LPARAM(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    fn active() -> TF_INPUTPROCESSORPROFILE {
        // SAFETY: COM object used on this test thread; the struct is plain data.
        unsafe {
            let profiles = profiles().expect("TSF profiles");
            let mut active: TF_INPUTPROCESSORPROFILE = std::mem::zeroed();
            profiles
                .GetActiveProfile(&GUID_TFCAT_TIP_KEYBOARD, &mut active)
                .unwrap();
            active
        }
    }

    fn pump() {
        // SAFETY: drains this thread's own messages.
        unsafe {
            let mut message = MSG::default();
            for _ in 0..50 {
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    DispatchMessageW(&message);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    /// Switches this test process to the system Korean IME, then puts back what was active.
    /// (Without the foreground hook: a foreground change meanwhile would restore early.)
    #[test]
    fn the_system_korean_ime_becomes_active_and_the_previous_one_comes_back() {
        // A TSF thread, as an app's UI thread is: switching only means something there.
        // SAFETY: COM on this test thread; the thread manager is deactivated before returning.
        let manager: ITfThreadMgr = unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER).unwrap()
        };
        let client = unsafe { manager.Activate().unwrap() };
        let before = active();
        switch_profile();
        pump();
        assert_eq!(
            active().clsid,
            SYSTEM_IME_CLSID,
            "after switch: {:?}",
            active().guidProfile
        );
        restore();
        pump();
        let after = active();
        let _ = unsafe { manager.Deactivate() };
        let _ = client;
        assert_eq!(
            (after.clsid, after.guidProfile),
            (before.clsid, before.guidProfile)
        );
    }
}
