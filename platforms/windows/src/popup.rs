//! Shared painting bits of the two small popup windows (the completion hint in ghost.rs and
//! the candidate list in candidates.rs): the theme palette, the DPI scale and the fonts.
//! Everything here is best effort; failures cost looks, never typing.

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

const FONT_FACE: windows::core::PCWSTR = w!("Segoe UI");

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: COLORREF,
    pub border: COLORREF,
    pub text: COLORREF,
    pub muted: COLORREF,
    /// The fill of the candidate window's selected line.
    pub selection: COLORREF,
}

const fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | (g << 8) | (b << 16))
}

const LIGHT: Palette = Palette {
    background: rgb(252, 252, 253),
    border: rgb(222, 222, 228),
    text: rgb(24, 24, 27),
    muted: rgb(140, 140, 150),
    selection: rgb(235, 238, 245),
};
const DARK: Palette = Palette {
    background: rgb(32, 32, 36),
    border: rgb(58, 58, 64),
    text: rgb(244, 244, 246),
    muted: rgb(140, 140, 152),
    selection: rgb(52, 52, 60),
};

/// The apps theme from Settings > Personalization > Colors (light when unreadable).
pub fn palette() -> Palette {
    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: `value` and `size` are valid for the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if status.is_ok() && value == 0 {
        DARK
    } else {
        LIGHT
    }
}

pub fn scale(hdc: HDC) -> f32 {
    // SAFETY: plain GDI query on a live DC.
    let dpi = unsafe { GetDeviceCaps(Some(hdc), LOGPIXELSY) };
    if dpi > 0 {
        dpi as f32 / 96.0
    } else {
        1.0
    }
}

pub fn px(logical: i32, scale: f32) -> i32 {
    (logical as f32 * scale).round() as i32
}

pub fn font(size: i32, weight: i32, scale: f32) -> HFONT {
    // SAFETY: plain GDI call; the caller deletes the font.
    unsafe {
        CreateFontW(
            -px(size, scale),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            FONT_FACE,
        )
    }
}

pub fn extent(hdc: HDC, text: &[u16]) -> SIZE {
    let mut size = SIZE::default();
    // SAFETY: live DC, valid slice.
    let _ = unsafe { GetTextExtentPoint32W(hdc, text, &mut size) };
    size
}
