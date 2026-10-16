//! The candidate window: the numbered list the open conversion offers, right under the word
//! being converted. A window of its own, like the completion hint (ghost.rs): it never takes
//! focus, and every failure costs only the list, never the typing.
//!
//! Looks: rounded, hairline border, no shadow; follows the app theme setting (light/dark);
//! sized in logical pixels and scaled by the screen's DPI. The selected line is filled, the
//! numbers sit in a muted leading column, and "2/3" shows under the list when the candidates
//! need more than one page. When the list would run off the monitor below the word, it flips
//! above it.

use std::cell::{Cell, RefCell};
use std::sync::Once;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::popup::{extent, font, palette, px, scale};
use typescript_core::session::CandidatePage;

/// Logical pixels at 96 dpi.
const LINE_HEIGHT: i32 = 28;
const PAD_X: i32 = 11;
const PAD_Y: i32 = 5;
/// Between the number column and the text.
const NUMBER_GAP: i32 = 10;
const RADIUS: i32 = 8;
/// Space between the word and the window.
const GAP: i32 = 4;
const TEXT_SIZE: i32 = 13;
const NUMBER_SIZE: i32 = 12;
const FOOTER_HEIGHT: i32 = 18;
const FOOTER_SIZE: i32 = 11;
const MIN_WIDTH: i32 = 110;
/// The selected fill, inset from the window's rounded edge.
const ROW_INSET: i32 = 4;

thread_local! {
    /// What the window paints; the window procedure has no other way to reach it.
    static PAGE: RefCell<Page> = RefCell::new(Page::default());
}

static REGISTER: Once = Once::new();

/// Painting-ready text for the page: the numbered lines and, for a list of several pages,
/// the "2/3" footer.
#[derive(Default)]
struct Page {
    rows: Vec<Row>,
    footer: Vec<u16>,
}

struct Row {
    number: Vec<u16>,
    text: Vec<u16>,
    selected: bool,
}

fn page_of(page: &CandidatePage) -> Page {
    let rows = page
        .texts
        .iter()
        .enumerate()
        .map(|(i, text)| Row {
            number: (i + 1).to_string().encode_utf16().collect(),
            text: text.encode_utf16().collect(),
            selected: i == page.selected,
        })
        .collect();
    let footer = if page.pages > 1 {
        format!("{}/{}", page.page + 1, page.pages)
            .encode_utf16()
            .collect()
    } else {
        Vec::new()
    };
    Page { rows, footer }
}

/// Where everything goes, in device pixels.
struct Layout {
    width: i32,
    height: i32,
    number_width: i32,
    rows_top: i32,
    row_height: i32,
    footer: Option<RECT>,
}

fn layout(hdc: HDC, page: &Page, scale: f32) -> Layout {
    // SAFETY: fonts are selected out of the DC and deleted before returning.
    unsafe {
        let text_font = font(TEXT_SIZE, 400, scale);
        let number_font = font(NUMBER_SIZE, 400, scale);
        let old = SelectObject(hdc, text_font.into());
        let text_width = page
            .rows
            .iter()
            .map(|row| extent(hdc, &row.text).cx)
            .max()
            .unwrap_or(0);
        SelectObject(hdc, number_font.into());
        let number_width = page
            .rows
            .iter()
            .map(|row| extent(hdc, &row.number).cx)
            .max()
            .unwrap_or(0)
            + px(NUMBER_GAP, scale);
        let footer_width = if page.footer.is_empty() {
            0
        } else {
            extent(hdc, &page.footer).cx
        };
        SelectObject(hdc, old);
        let _ = DeleteObject(text_font.into());
        let _ = DeleteObject(number_font.into());

        let row_height = px(LINE_HEIGHT, scale);
        let rows_height = row_height * page.rows.len() as i32;
        let footer_height = if page.footer.is_empty() {
            0
        } else {
            px(FOOTER_HEIGHT, scale)
        };
        let width = (px(PAD_X, scale) * 2 + number_width + text_width)
            .max(px(MIN_WIDTH, scale))
            .max(footer_width + px(PAD_X, scale) * 2);
        let footer_top = px(PAD_Y, scale) + rows_height;
        Layout {
            width,
            height: px(PAD_Y, scale) * 2 + rows_height + footer_height,
            number_width,
            rows_top: px(PAD_Y, scale),
            row_height,
            footer: (footer_height > 0).then(|| RECT {
                left: 0,
                top: footer_top,
                right: width,
                bottom: footer_top + footer_height,
            }),
        }
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let colors = palette();
    let scale = scale(hdc);
    let page = PAGE.with(|p| {
        let page = p.borrow();
        Page {
            rows: page
                .rows
                .iter()
                .map(|row| Row {
                    number: row.number.clone(),
                    text: row.text.clone(),
                    selected: row.selected,
                })
                .collect(),
            footer: page.footer.clone(),
        }
    });
    let l = layout(hdc, &page, scale);
    let radius = px(RADIUS, scale);

    // Background first; the window region already rounds the corners.
    let background = CreateSolidBrush(colors.background);
    let area = RECT {
        left: 0,
        top: 0,
        right: l.width,
        bottom: l.height,
    };
    FillRect(hdc, &area, background);
    let _ = DeleteObject(background.into());

    SetBkMode(hdc, TRANSPARENT);
    let text_font = font(TEXT_SIZE, 400, scale);
    let number_font = font(NUMBER_SIZE, 400, scale);
    for (i, row) in page.rows.iter().enumerate() {
        let top = l.rows_top + l.row_height * i as i32;
        if row.selected {
            let fill = CreateSolidBrush(colors.selection);
            let row_area = RECT {
                left: px(ROW_INSET, scale),
                top,
                right: l.width - px(ROW_INSET, scale),
                bottom: top + l.row_height,
            };
            FillRect(hdc, &row_area, fill);
            let _ = DeleteObject(fill.into());
        }
        let old_font = SelectObject(hdc, number_font.into());
        SetTextColor(hdc, colors.muted);
        let number_size = extent(hdc, &row.number);
        let _ = TextOutW(
            hdc,
            px(PAD_X, scale),
            top + (l.row_height - number_size.cy) / 2,
            &row.number,
        );
        SelectObject(hdc, text_font.into());
        SetTextColor(hdc, colors.text);
        let text_size = extent(hdc, &row.text);
        let _ = TextOutW(
            hdc,
            px(PAD_X, scale) + l.number_width,
            top + (l.row_height - text_size.cy) / 2,
            &row.text,
        );
        SelectObject(hdc, old_font);
    }

    if let Some(mut footer) = l.footer {
        let footer_font = font(FOOTER_SIZE, 400, scale);
        let old_font = SelectObject(hdc, footer_font.into());
        SetTextColor(hdc, colors.muted);
        let mut text = page.footer.clone();
        DrawTextW(
            hdc,
            &mut text,
            &mut footer,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(hdc, old_font);
        let _ = DeleteObject(footer_font.into());
    }

    // The hairline border, over the fills.
    let border = CreatePen(PS_SOLID, 1, colors.border);
    let old_pen = SelectObject(hdc, border.into());
    let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
    let _ = RoundRect(hdc, 0, 0, l.width - 1, l.height - 1, radius * 2, radius * 2);
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    let _ = DeleteObject(border.into());
    let _ = DeleteObject(text_font.into());
    let _ = DeleteObject(number_font.into());
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

/// One per text service (thread); the window is made on the first list.
#[derive(Default)]
pub struct CandidateWindow {
    hwnd: Cell<HWND>,
}

impl CandidateWindow {
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
                    lpszClassName: w!("TypescriptCandidates"),
                    ..Default::default()
                };
                RegisterClassW(&class);
            });
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                w!("TypescriptCandidates"),
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

    /// Shows `page` under the word (`anchor`, its rectangle on screen), flipping above it
    /// when the list would run off the monitor's work area.
    pub fn show(&self, page: &CandidatePage, anchor: RECT) {
        let Some(hwnd) = self.window() else { return };
        let data = page_of(page);
        // SAFETY: `hwnd` is this thread's window; the DC is released before returning.
        unsafe {
            let hdc = GetDC(Some(hwnd));
            let scale = scale(hdc);
            let l = layout(hdc, &data, scale);
            ReleaseDC(Some(hwnd), hdc);
            PAGE.with(|p| *p.borrow_mut() = data);
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
            let mut top = anchor.bottom + px(GAP, scale);
            let monitor = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info).as_bool() && top + l.height > info.rcWork.bottom
            {
                top = anchor.top - px(GAP, scale) - l.height;
            }
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                anchor.left,
                top,
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
    fn shows_paints_and_hides_the_candidate_window() {
        let window = CandidateWindow::default();
        let anchor = RECT {
            left: 100,
            top: 100,
            right: 160,
            bottom: 120,
        };
        let page = CandidatePage {
            texts: (1..=12).map(|i| format!("候補{i}")).collect(),
            selected: 1,
            page: 0,
            pages: 2,
        };
        window.show(&page, anchor);
        let hwnd = window.hwnd.get();
        assert!(!hwnd.is_invalid());
        // SAFETY: this thread's window; the message loop only drains what the window posted.
        unsafe {
            assert!(IsWindowVisible(hwnd).as_bool());
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect).unwrap();
            // Just under the word, left-aligned with it; nine lines and the footer.
            assert_eq!(rect.left, 100);
            assert!(rect.top >= 120 && rect.top < 130, "top {}", rect.top);
            assert!(rect.right - rect.left >= 110);
            assert!(
                rect.bottom - rect.top > 240,
                "height {}",
                rect.bottom - rect.top
            );
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
        }
        window.hide();
        assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
        window.destroy();
        assert!(window.hwnd.get().is_invalid());
    }
}
