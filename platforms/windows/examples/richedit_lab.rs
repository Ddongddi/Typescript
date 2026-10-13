//! A lab for the KakaoTalk problem: KakaoTalk's chat input is a plain `RICHEDIT50W`, so this
//! puts the same control in a window, switches this process to a text service, types a word with
//! real key events and prints what the control ended up with.
//!
//!   cargo run -p typescript-tip --example richedit_lab -- --ms      Microsoft's Korean IME (works in KakaoTalk)
//!   cargo run -p typescript-tip --example richedit_lab -- --ts      Typescript (the installed one)
//!
//! Keys go out with SendInput, so the window must be in front; the lab refuses to type
//! otherwise. Needs the text service to be installed (scripts\install.ps1).

use std::time::{Duration, Instant};

use windows::core::{w, GUID};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Input::Ime::{
    ImmGetCompositionStringW, ImmGetContext, ImmGetConversionStatus, ImmGetOpenStatus, ImmIsIME,
    ImmReleaseContext, GCS_COMPATTR, GCS_COMPSTR, GCS_CURSORPOS,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::TextServices::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const LANGID_KO: u16 = 0x0412;
const MS_IME: (GUID, GUID) = (
    GUID::from_u128(0xA028AE76_01B1_46C2_99C4_ACD9858AE02F),
    GUID::from_u128(0xB5FE1F02_D5F2_4445_9C03_C568F23C99A1),
);
const TYPESCRIPT: (GUID, GUID) = (
    GUID::from_u128(0xE231DAD4_326F_436F_B74C_D08F1D66D822),
    GUID::from_u128(0xD7B9871C_E74A_4B4F_9791_CBD1E9E074FE),
);

static mut OLD_PROC: isize = 0;

/// Logs the IME messages the RichEdit gets, then lets it handle them.
unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let name = match message {
        0x010D => Some("STARTCOMPOSITION"),
        0x010E => Some("ENDCOMPOSITION"),
        0x010F => Some("COMPOSITION"),
        0x0282 => Some("NOTIFY"),
        0x0286 => Some("CHAR"),
        0x0288 => Some("REQUEST"),
        _ => None,
    };
    if let Some(name) = name {
        println!(
            "    WM_IME_{name} wparam={:#x} lparam={:#x}",
            wparam.0, lparam.0
        );
    }
    let old: WNDPROC = std::mem::transmute(OLD_PROC);
    CallWindowProcW(old, hwnd, message, wparam, lparam)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, message, wparam, lparam)
}

fn pump(duration: Duration) {
    let end = Instant::now() + duration;
    while Instant::now() < end {
        // SAFETY: draining this thread's own messages.
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn text_of(control: HWND) -> String {
    let mut buffer = [0u16; 256];
    // SAFETY: buffer valid for its length.
    let n = unsafe { GetWindowTextW(control, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..n])
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    let (clsid, profile) = match which.as_str() {
        "--ms" => MS_IME,
        "--ts" => TYPESCRIPT,
        _ => {
            eprintln!("usage: richedit_lab --ms | --ts");
            std::process::exit(2);
        }
    };
    // SAFETY: a single-threaded Win32 program; every handle is used on this thread.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ = LoadLibraryW(w!("Msftedit.dll"));
        let instance = HINSTANCE(GetModuleHandleW(None).unwrap().0);
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: w!("RichEditLab"),
            hbrBackground: HBRUSH_WHITE,
            ..Default::default()
        };
        RegisterClassW(&class);
        let window = CreateWindowExW(
            WS_EX_TOPMOST,
            w!("RichEditLab"),
            w!("richedit lab (do not type)"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            200,
            200,
            560,
            220,
            None,
            None,
            Some(instance),
            None,
        )
        .expect("window");
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RICHEDIT50W"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(0x0004 /* ES_MULTILINE */),
            10,
            10,
            520,
            140,
            Some(window),
            None,
            Some(instance),
            None,
        )
        .expect("RICHEDIT50W (is Msftedit.dll available?)");

        OLD_PROC = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
        // Bring the lab to the front and give the edit the focus.
        let _ = ShowWindow(window, SW_SHOW);
        // Windows only lets the foreground thread hand over the foreground: borrow its input.
        let front = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = windows::Win32::System::Threading::GetCurrentThreadId();
        let _ = windows::Win32::System::Threading::AttachThreadInput(me, front, true);
        let _ = SetForegroundWindow(window);
        let _ = windows::Win32::System::Threading::AttachThreadInput(me, front, false);
        let _ = SetFocus(Some(edit));
        pump(Duration::from_millis(300));

        // Microsoft's IME is Korean; Typescript is registered under en-US (see guids.rs).
        // LAB_LANGID=0412 tries another language the profile is registered under.
        let default_langid = if which == "--ms" { LANGID_KO } else { 0x0409 };
        let langid = std::env::var("LAB_LANGID")
            .ok()
            .and_then(|v| u16::from_str_radix(&v, 16).ok())
            .unwrap_or(default_langid);
        // LAB_PROFILE=7A3F0001-0000-4000-8000-000000000001: another (lab) profile of the service.
        let profile = std::env::var("LAB_PROFILE")
            .ok()
            .and_then(|v| u128::from_str_radix(&v.replace('-', ""), 16).ok())
            .map_or(profile, GUID::from_u128);
        // A TSF thread, then the chosen text service for this process.
        let manager: ITfThreadMgr =
            CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER).unwrap();
        let _client = manager.Activate().unwrap();
        let profiles: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER).unwrap();
        profiles
            .ActivateProfile(
                1,
                langid,
                &clsid,
                &profile,
                HKL::default(),
                TF_IPPMF_FORPROCESS | TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
            )
            .unwrap();
        pump(Duration::from_millis(800));
        let _ = SetFocus(Some(edit));
        pump(Duration::from_millis(200));
        if GetForegroundWindow() != window {
            eprintln!("the lab window is not in front: not typing (nothing was sent)");
            return;
        }
        // Microsoft's IME starts in English mode: the Han/Eng key turns Hangul on.
        if which == "--ms" {
            let inputs = [key(0x15, false), key(0x15, true)];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            pump(Duration::from_millis(400));
        }
        // What an IMM32 app can ask the IME right now (RichEdit and KakaoTalk do).
        let layout = GetKeyboardLayout(0);
        let himc = ImmGetContext(edit);
        let (mut conversion, mut sentence) = (Default::default(), Default::default());
        let have_status =
            ImmGetConversionStatus(himc, Some(&mut conversion), Some(&mut sentence)).as_bool();
        println!(
            "{which}: hkl={:#010x} is_ime={} open={} conversion={:#x} sentence={:#x} (status ok: {have_status})",
            layout.0 as usize,
            ImmIsIME(layout).as_bool(),
            ImmGetOpenStatus(himc).as_bool(),
            conversion.0,
            sentence.0
        );
        let _ = ImmReleaseContext(edit, himc);
        // x p t m x m = 테스트 on a 2-set Korean layout.
        for vk in *b"XPTMXM" {
            if GetForegroundWindow() != window {
                eprintln!("lost the foreground: stopped");
                break;
            }
            let inputs = [key(vk as u16, false), key(vk as u16, true)];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            pump(Duration::from_millis(120));
            // What the IMM context holds as the composition right now (GCS_COMPSTR = 8, GCS_COMPATTR = 0x10).
            let himc = ImmGetContext(edit);
            let compstr = ImmGetCompositionStringW(himc, GCS_COMPSTR, None, 0);
            let mut attr = [0u8; 8];
            let compattr =
                ImmGetCompositionStringW(himc, GCS_COMPATTR, Some(attr.as_mut_ptr().cast()), 8);
            let cursor = ImmGetCompositionStringW(himc, GCS_CURSORPOS, None, 0);
            println!(
                "  after {}: IMM composition bytes={compstr} attr={:?} cursor={cursor}",
                vk as char,
                &attr[..compattr.max(0) as usize]
            );
            let _ = ImmReleaseContext(edit, himc);
        }
        pump(Duration::from_millis(400));
        let installed =
            GetModuleHandleW(w!(r"C:\Program Files\Typescript\typescript_tip.dll")).is_ok();
        println!("{which}: installed DLL loaded: {installed}");
        let typed = text_of(edit);
        println!("{which}: the control holds {typed:?}  (wanted \"테스트\" after Space)");
        // Space ends the word.
        let inputs = [key(VK_SPACE.0, false), key(VK_SPACE.0, true)];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        pump(Duration::from_millis(400));
        println!("{which}: after Space: {:?}", text_of(edit));
        let _ = manager.Deactivate();
    }
}

const HBRUSH_WHITE: windows::Win32::Graphics::Gdi::HBRUSH =
    windows::Win32::Graphics::Gdi::HBRUSH(6 as *mut core::ffi::c_void); // COLOR_WINDOW + 1
