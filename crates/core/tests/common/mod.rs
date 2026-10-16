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

/// The keys that type `text`: syllables as on a 2-set layout, every other character as it is.
pub fn keys_of_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '가'..='힣' => keys_of(&c.to_string()),
            other => other.to_string(),
        })
        .collect()
}

/// Levenshtein distance in characters.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != *cb))
                .min(up + 1)
                .min(row[j] + 1);
            diagonal = up;
        }
    }
    row[b.len()]
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
