//! Identifiers of the text service. Generated once for this project: never change them, they
//! are what Windows has registered.

use windows::core::GUID;

/// COM class of the text input processor.
pub const CLSID_TYPESCRIPT: GUID = GUID::from_u128(0xE231DAD4_326F_436F_B74C_D08F1D66D822);
/// Profile with Tab completion (the first profile ever registered: its GUID stays).
pub const GUID_PROFILE: GUID = GUID::from_u128(0xD7B9871C_E74A_4B4F_9791_CBD1E9E074FE);
/// Profile without Tab completion (and without counting the words typed).
pub const GUID_PROFILE_PLAIN: GUID = GUID::from_u128(0xBFC1C2A9_657F_443A_B4DE_ACF8EACA9AEC);
/// The display attribute (dotted underline) of the text being composed.
pub const GUID_DISPLAY_ATTRIBUTE: GUID = GUID::from_u128(0x268841E3_D0E9_45B7_80EC_783286D96160);

/// en-US: the language Typescript is registered under. Not Korean on purpose: for a Korean
/// text service, Windows' bridge to IMM32 apps (KakaoTalk, anything built on RichEdit) only
/// keeps a one-character "interim" composition and commits the rest at once, so the preview of
/// a whole word broke into jamo and letters (ㅌpㅅmㅌm). Under any other language the bridge
/// keeps the whole composition. (Pairing Korean with a US keyboard layout does not help: the
/// bridge goes by the profile's language.)
pub const LANGID_TYPESCRIPT: u16 = 0x0409;
/// ko-KR: Microsoft's Korean IME (see delegate.rs), and where Typescript used to be registered.
pub const LANGID_KO_KR: u16 = 0x0412;
pub const DESCRIPTION: &str = "Typescript";

/// The profiles the input picker offers: (GUID, name, with Tab completion).
pub const PROFILES: [(GUID, &str, bool); 2] = [
    (GUID_PROFILE, "Typescript (자동완성)", true),
    (GUID_PROFILE_PLAIN, "Typescript", false),
];

/// `{E231DAD4-326F-436F-B74C-D08F1D66D822}`, the registry spelling.
pub fn guid_string(g: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1, g.data2, g.data3, g.data4[0], g.data4[1], g.data4[2], g.data4[3], g.data4[4], g.data4[5], g.data4[6],
        g.data4[7]
    )
}

/// NUL-terminated UTF-16.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
