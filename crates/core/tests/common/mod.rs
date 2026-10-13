//! Helpers shared by the integration tests.

#![allow(dead_code)] // each test binary uses a different subset

use typescript_core::lang::hangul::DubeolsikLayout;

const CHO: &str = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ";
const JUNG: [&str; 21] = [
    "ㅏ", "ㅐ", "ㅑ", "ㅒ", "ㅓ", "ㅔ", "ㅕ", "ㅖ", "ㅗ", "ㅗㅏ", "ㅗㅐ", "ㅗㅣ", "ㅛ", "ㅜ",
    "ㅜㅓ", "ㅜㅔ", "ㅜㅣ", "ㅠ", "ㅡ", "ㅡㅣ", "ㅣ",
];
const JONG: [&str; 28] = [
    "", "ㄱ", "ㄲ", "ㄱㅅ", "ㄴ", "ㄴㅈ", "ㄴㅎ", "ㄷ", "ㄹ", "ㄹㄱ", "ㄹㅁ", "ㄹㅂ", "ㄹㅅ",
    "ㄹㅌ", "ㄹㅍ", "ㄹㅎ", "ㅁ", "ㅂ", "ㅂㅅ", "ㅅ", "ㅆ", "ㅇ", "ㅈ", "ㅊ", "ㅋ", "ㅌ", "ㅍ",
    "ㅎ",
];

fn key_of(jamo: char) -> char {
    ('A'..='z')
        .find(|k| {
            DubeolsikLayout::jamo(*k) == Some(jamo) && (k.is_lowercase() || "QWERTOP".contains(*k))
        })
        .unwrap()
}

/// The keys that type `hangul` on a 2-set layout.
pub fn keys_of(hangul: &str) -> String {
    let mut keys = String::new();
    for c in hangul.chars() {
        let i = c as usize - '가' as usize;
        let cho = CHO.chars().nth(i / 588).unwrap();
        keys.push(key_of(cho));
        JUNG[i % 588 / 28]
            .chars()
            .chain(JONG[i % 28].chars())
            .for_each(|j| keys.push(key_of(j)));
    }
    keys
}
