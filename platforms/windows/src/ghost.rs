//! The completion hint: a small flat pill under the word being typed, showing what Tab would
//! complete it to (`안녕하세요  [Tab]`). A window of its own, not text in the composition: if an
//! app ends the composition itself (a click elsewhere), a hint inside the text would be left in
//! the document, while this just stays unseen. Every failure here only costs the hint.
//!
//! Looks: rounded, hairline border, no shadow; follows the app theme setting (light/dark);
//! sized in logical pixels and scaled by the screen's DPI.

use std::cell::{Cell, RefCell};
use std::sync::Once;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::popup::{extent, font, palette, px, scale};

/// Logical pixels at 96 dpi.
const HEIGHT: i32 = 28;
const PAD_X: i32 = 11;
const KEY_GAP: i32 = 10;
const KEY_PAD_X: i32 = 6;
const KEY_HEIGHT: i32 = 16;
const RADIUS: i32 = 8;
/// Space between the word and the pill.
const GAP: i32 = 4;
const TEXT_SIZE: i32 = 13;
const KEY_SIZE: i32 = 10;

thread_local! {
    /// What the window paints; the window procedure has no other way to reach it.
    static WORD: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

static REGISTER: Once = Once::new();

/// Where everything goes, in device pixels.
struct Layout {
    width: i32,
    height: i32,
    word_height: i32,
    key: RECT,
}

fn layout(hdc: HDC, word: &[u16], scale: f32) -> Layout {
    // SAFETY: fonts are selected out of the DC and deleted before returning.
    unsafe {
        let text_font = font(TEXT_SIZE, 400, scale);
        let key_font = font(KEY_SIZE, 600, scale);
        let old = SelectObject(hdc, text_font.into());
        let word_size = extent(hdc, word);
        SelectObject(hdc, key_font.into());
        let key_text = extent(hdc, &"Tab".encode_utf16().collect::<Vec<_>>());
        SelectObject(hdc, old);
        let _ = DeleteObject(text_font.into());
        let _ = DeleteObject(key_font.into());

        let height = px(HEIGHT, scale);
        let key_width = key_text.cx + px(KEY_PAD_X * 2, scale);
        let key_height = px(KEY_HEIGHT, scale);
        let key_left = px(PAD_X, scale) + word_size.cx + px(KEY_GAP, scale);
        let key_top = (height - key_height) / 2;
        Layout {
            width: key_left + key_width + px(PAD_X - 3, scale),
            height,
            word_height: word_size.cy,
            key: RECT {
                left: key_left,
                top: key_top,
                right: key_left + key_width,
                bottom: key_top + key_height,
            },
        }
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let colors = palette();
    let scale = scale(hdc);
    let word = WORD.with(|w| w.borrow().clone());
    let l = layout(hdc, &word, scale);
    let radius = px(RADIUS, scale);

    // Background, then the hairline border (the window region already rounds the corners).
    let background = CreateSolidBrush(colors.background);
    let area = RECT {
        left: 0,
        top: 0,
        right: l.width,
        bottom: l.height,
    };
    FillRect(hdc, &area, background);
    let _ = DeleteObject(background.into());
    let border = CreatePen(PS_SOLID, 1, colors.border);
    let old_pen = SelectObject(hdc, border.into());
    let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
    let _ = RoundRect(hdc, 0, 0, l.width - 1, l.height - 1, radius * 2, radius * 2);

    SetBkMode(hdc, TRANSPARENT);
    // The word.
    let text_font = font(TEXT_SIZE, 400, scale);
    let old_font = SelectObject(hdc, text_font.into());
    SetTextColor(hdc, colors.text);
    let _ = TextOutW(hdc, px(PAD_X, scale), (l.height - l.word_height) / 2, &word);

    // The key cap: an outlined pill with a muted "Tab".
    let cap_pen = CreatePen(PS_SOLID, 1, colors.border);
    SelectObject(hdc, cap_pen.into());
    let _ = RoundRect(
        hdc,
        l.key.left,
        l.key.top,
        l.key.right,
        l.key.bottom,
        px(8, scale),
        px(8, scale),
    );
    let key_font = font(KEY_SIZE, 600, scale);
    SelectObject(hdc, key_font.into());
    SetTextColor(hdc, colors.muted);
    let mut label: Vec<u16> = "Tab".encode_utf16().collect();
    let mut key = l.key;
    DrawTextW(
        hdc,
        &mut label,
        &mut key,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    SelectObject(hdc, old_font);
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    let _ = DeleteObject(text_font.into());
    let _ = DeleteObject(key_font.into());
    let _ = DeleteObject(border.into());
    let _ = DeleteObject(cap_pen.into());
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1), // painted in full
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
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
                    lpszClassName: w!("tacitCompletionHint"),
                    ..Default::default()
                };
                RegisterClassW(&class);
            });
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                w!("tacitCompletionHint"),
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

    /// Shows the completion `word` just below `anchor`, the typed word's rectangle on screen.
    pub fn show(&self, word: &str, anchor: RECT) {
        let Some(hwnd) = self.window() else { return };
        let chars: Vec<u16> = word.encode_utf16().collect();
        // SAFETY: `hwnd` is this thread's window; the DC is released before returning.
        unsafe {
            let hdc = GetDC(Some(hwnd));
            let scale = scale(hdc);
            let l = layout(hdc, &chars, scale);
            ReleaseDC(Some(hwnd), hdc);
            WORD.with(|w| *w.borrow_mut() = chars);
            let radius = px(RADIUS, scale) * 2;
            // +1: the region excludes its right/bottom edge.
            SetWindowRgn(
                hwnd,
                Some(CreateRoundRectRgn(
                    0,
                    0,
                    l.width + 1,
                    l.height + 1,
                    radius,
                    radius,
                )),
                true,
            );
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                anchor.left,
                anchor.bottom + px(GAP, scale),
                l.width,
                l.height,
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
        let anchor = RECT {
            left: 100,
            top: 100,
            right: 160,
            bottom: 120,
        };
        ghost.show("안녕하세요", anchor);
        let hwnd = ghost.hwnd.get();
        assert!(!hwnd.is_invalid());
        // SAFETY: this thread's window; the message loop only drains what the window posted.
        unsafe {
            assert!(IsWindowVisible(hwnd).as_bool());
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect).unwrap();
            // Just under the word, left-aligned with it, a pill wider than tall.
            assert_eq!(rect.left, 100);
            assert!(rect.top > 120 && rect.top < 130, "top {}", rect.top);
            assert!(rect.right - rect.left > rect.bottom - rect.top);
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
