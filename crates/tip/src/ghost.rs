//! The completion hint: a small tool window under the word being typed, showing what Tab
//! would complete it to. A window of its own, not text in the composition: if an app ends the
//! composition itself (a click elsewhere), a hint inside the text would be left in the
//! document, while this just stays unseen. Every failure here only costs the hint.

use std::cell::{Cell, RefCell};
use std::sync::Once;

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const PADDING: i32 = 6;
const GAP: i32 = 2;

thread_local! {
    /// What the window paints; the window procedure has no other way to reach it.
    static TEXT: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

static REGISTER: Once = Once::new();

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_PAINT {
        let mut paint = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut paint);
        let mut area = RECT::default();
        let _ = GetClientRect(hwnd, &mut area);
        FillRect(hdc, &area, GetSysColorBrush(COLOR_INFOBK));
        FrameRect(hdc, &area, GetSysColorBrush(COLOR_WINDOWFRAME));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(GetSysColor(COLOR_INFOTEXT)));
        let old = SelectObject(hdc, GetStockObject(DEFAULT_GUI_FONT));
        TEXT.with(|t| {
            let _ = TextOutW(hdc, PADDING, PADDING / 2 + 1, &t.borrow());
        });
        SelectObject(hdc, old);
        let _ = EndPaint(hwnd, &paint);
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

/// One per text service (thread); the window is made on the first hint.
#[derive(Default)]
pub struct Ghost {
    hwnd: Cell<HWND>,
}

impl Ghost {
    fn window(&self) -> Option<HWND> {
        if !self.hwnd.get().is_invalid() {
            return Some(self.hwnd.get());
        }
        // SAFETY: plain Win32 calls with a class and window owned by this thread.
        unsafe {
            let instance = HINSTANCE(crate::module_handle().0);
            REGISTER.call_once(|| {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance,
                    lpszClassName: w!("TypescriptCompletionHint"),
                    ..Default::default()
                };
                RegisterClassW(&class);
            });
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                w!("TypescriptCompletionHint"),
                None,
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance),
                None,
            )
            .ok()?;
            self.hwnd.set(hwnd);
            Some(hwnd)
        }
    }

    /// Shows `text` just below `word`, the word's rectangle on screen.
    pub fn show(&self, text: &str, word: RECT) {
        let Some(hwnd) = self.window() else { return };
        let chars: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: `hwnd` is this thread's window; the DC is released before returning.
        unsafe {
            let hdc = GetDC(Some(hwnd));
            let old = SelectObject(hdc, GetStockObject(DEFAULT_GUI_FONT));
            let mut size = Default::default();
            let _ = GetTextExtentPoint32W(hdc, &chars, &mut size);
            SelectObject(hdc, old);
            ReleaseDC(Some(hwnd), hdc);
            TEXT.with(|t| *t.borrow_mut() = chars);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                word.left,
                word.bottom + GAP,
                size.cx + PADDING * 2,
                size.cy + PADDING,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(Some(hwnd), None, true);
        }
    }

    pub fn hide(&self) {
        if !self.hwnd.get().is_invalid() {
            // SAFETY: this thread's window.
            let _ = unsafe { ShowWindow(self.hwnd.get(), SW_HIDE) };
        }
    }

    pub fn destroy(&self) {
        let hwnd = self.hwnd.replace(HWND::default());
        if !hwnd.is_invalid() {
            // SAFETY: this thread's window.
            let _ = unsafe { DestroyWindow(hwnd) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the window through show, paint, hide and destroy on a real (desktop) window.
    #[test]
    fn shows_paints_and_hides_the_hint_window() {
        let ghost = Ghost::default();
        let word = RECT { left: 100, top: 100, right: 160, bottom: 120 };
        ghost.show("안녕하세요   Tab", word);
        let hwnd = ghost.hwnd.get();
        assert!(!hwnd.is_invalid());
        // SAFETY: this thread's window; the message loop only drains what the window posted.
        unsafe {
            assert!(IsWindowVisible(hwnd).as_bool());
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect).unwrap();
            assert_eq!((rect.left, rect.top), (100, 120 + GAP));
            assert!(rect.right - rect.left > PADDING * 2);
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
        }
        ghost.hide();
        assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
        ghost.destroy();
        assert!(ghost.hwnd.get().is_invalid());
    }
}
