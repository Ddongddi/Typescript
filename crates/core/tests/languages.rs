//! Any combination of languages, including one: decisions only ever come out in a language
//! that is on.

use typescript_core::{make_engine, DecisionContext, DecisionEngine, Language, Languages};

fn decide(ids: &[&str], keys: &str) -> (Language, String) {
    let engine = make_engine(None, &Languages::from_ids(ids).unwrap());
    let d = engine.decide(keys, &DecisionContext::default());
    (d.language, d.output)
}

#[test]
fn korean_only_reads_everything_as_korean() {
    assert_eq!(decide(&["ko"], "dkssud"), (Language::Korean, "안녕".into()));
    assert_eq!(decide(&["ko"], "hello").0, Language::Korean);
    assert_eq!(decide(&["ko"], "githubdp").0, Language::Korean); // no Mixed without English
}

#[test]
fn english_only_never_converts() {
    for keys in ["dkssud", "githubdp", "hello", "rkskek"] {
        assert_eq!(decide(&["en"], keys), (Language::English, keys.to_string()));
    }
}

#[test]
fn english_and_korean_keep_mixed() {
    assert_eq!(
        decide(&["en", "ko"], "githubdp"),
        (Language::Mixed, "github에".into())
    );
}

#[test]
fn order_does_not_change_the_decision() {
    assert_eq!(
        decide(&["ko", "en"], "dkssud"),
        decide(&["en", "ko"], "dkssud")
    );
    assert_eq!(
        decide(&["ko", "en"], "hello"),
        decide(&["en", "ko"], "hello")
    );
}

#[test]
fn uppercase_without_english_is_scored_not_forced_english() {
    assert_eq!(decide(&["ko"], "Hello").0, Language::Korean);
}

#[test]
fn learned_language_that_is_off_is_ignored() {
    let path = std::env::temp_dir().join(format!("ts-langs-learn-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let both = make_engine(
        Some(path.clone()),
        &Languages::from_ids(&["en", "ko"]).unwrap(),
    );
    both.learn("hello", Language::Korean, Some(Language::English));
    both.learn("hello", Language::Korean, Some(Language::English));
    assert_eq!(
        both.decide("hello", &DecisionContext::default()).language,
        Language::Korean
    );
    let english = make_engine(Some(path.clone()), &Languages::from_ids(&["en"]).unwrap());
    assert_eq!(
        english
            .decide("hello", &DecisionContext::default())
            .language,
        Language::English
    );
    let _ = std::fs::remove_file(&path);
}

#[cfg(feature = "japanese")]
mod japanese {
    use super::*;

    #[test]
    fn japanese_only_reads_romaji_as_kana() {
        assert_eq!(
            decide(&["ja"], "kyou"),
            (Language::Japanese, "きょう".into())
        );
    }

    #[test]
    fn korean_and_japanese_without_english() {
        assert_eq!(decide(&["ko", "ja"], "dkssudgktpdy").0, Language::Korean);
        assert_eq!(
            decide(&["ko", "ja"], "kyouhaiitenkidesune").0,
            Language::Japanese
        );
    }

    #[test]
    fn english_and_japanese_without_korean() {
        assert_eq!(decide(&["en", "ja"], "dkssud").0, Language::English);
        assert_eq!(decide(&["en", "ja"], "githubdp").0, Language::English);
    }
}
