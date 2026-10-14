//! Every language module and combination rule this build knows. A new language: a folder
//! under `lang/`, a `Language` variant, and a line here.

use crate::decide::engine::Language;
use crate::lang::module::{ComboRule, LanguageModule};
use crate::lang::set::Languages;

static MODULES: [&dyn LanguageModule; 3] = [
    &crate::lang::en::English,
    &crate::lang::ko::Korean,
    &crate::lang::ja::Japanese,
];

static COMBOS: [&dyn ComboRule; 1] = [&crate::lang::combo::en_ko::EnglishKorean];

pub fn module(language: Language) -> Option<&'static dyn LanguageModule> {
    MODULES.iter().copied().find(|m| m.language() == language)
}

pub fn combo(language: Language) -> Option<&'static dyn ComboRule> {
    COMBOS.iter().copied().find(|c| c.language() == language)
}

/// The text `keys` type in `language`, from its module or its combination rule.
pub fn reading(language: Language, keys: &str) -> Option<String> {
    match module(language) {
        Some(m) => m.compose(keys),
        None => combo(language).and_then(|c| c.compose(keys)),
    }
}

/// Combination readings count as clean: each rule already asks for clean halves.
pub fn is_clean(language: Language, keys: &str) -> bool {
    module(language).is_none_or(|m| m.is_clean(keys))
}

/// The order Esc visits readings in: the user's languages, each combination reading right
/// after the first language it needs.
pub fn cycle_order(languages: &Languages) -> Vec<Language> {
    let mut order = Vec::new();
    for language in languages.iter() {
        order.push(language);
        for c in COMBOS.iter() {
            let [first, second] = c.needs();
            if first == language && languages.contains(second) {
                order.push(c.language());
            }
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use Language::*;

    fn langs(ids: &[&str]) -> Languages {
        Languages::from_ids(ids).unwrap()
    }

    #[test]
    fn cycle_order_puts_mixed_after_english() {
        assert_eq!(
            cycle_order(&langs(&["en", "ko", "ja"])),
            vec![English, Mixed, Korean, Japanese]
        );
        assert_eq!(
            cycle_order(&langs(&["ko", "en"])),
            vec![Korean, English, Mixed]
        );
        assert_eq!(cycle_order(&langs(&["ko", "ja"])), vec![Korean, Japanese]);
        assert_eq!(cycle_order(&langs(&["en"])), vec![English]);
    }

    #[test]
    fn readings_come_from_the_modules() {
        assert_eq!(reading(English, "dkssud").as_deref(), Some("dkssud"));
        assert_eq!(reading(Korean, "dkssud").as_deref(), Some("안녕"));
        assert_eq!(reading(Japanese, "kyou").as_deref(), Some("きょう"));
        assert!(is_clean(Korean, "dkssud"));
        assert!(is_clean(Japanese, "kyou"));
        assert!(!is_clean(Japanese, "dkssud"));
    }

    #[test]
    fn one_language_never_cycles() {
        let ko = langs(&["ko"]);
        assert_eq!(Korean.next("dkssud", &ko), Korean);
        assert_eq!(English.next("dkssud", &ko), Korean);
    }

    #[test]
    fn cycle_follows_the_users_order() {
        let ko_en = langs(&["ko", "en"]);
        assert_eq!(Korean.next("dkssud", &ko_en), English);
        assert_eq!(English.next("dkssud", &ko_en), Korean);
        assert_eq!(English.next("dkssud", &langs(&["en"])), English);
    }

    #[test]
    fn a_lone_consonant_stays_in_the_cycle() {
        let all = langs(&["en", "ko", "ja"]);
        assert_eq!(Korean.next("a", &all), Japanese);
        assert_eq!(Japanese.next("a", &all), English);
        assert_eq!(English.next("a", &all), Korean);
    }
}
