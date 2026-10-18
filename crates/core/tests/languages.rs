//! Any combination of languages, including one: decisions only ever come out in a language
//! that is on.

use tacit_core::{make_engine, DecisionContext, DecisionEngine, Language, Languages};

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

mod session {
    use tacit_core::session::testing::FakeTextField;
    use tacit_core::{make_engine, AppPolicies, InputSession, Key, Languages, TextClient};

    fn typed(ids: &[&str], script: &str) -> String {
        let engine = make_engine(None, &Languages::from_ids(ids).unwrap());
        let mut session = InputSession::new(engine, AppPolicies::default(), true);
        let mut field = FakeTextField::new();
        for ch in script.chars() {
            let key = match ch {
                '⎋' => Key::Escape,
                c if c.is_ascii_alphabetic() => Key::Letter(c),
                c => Key::Printable(c),
            };
            if !session.handle(key, &mut field) {
                if let Key::Printable(c) = key {
                    field.insert_text(&c.to_string(), None);
                }
            }
        }
        session.commit(&mut field);
        field.text()
    }

    #[test]
    fn korean_only_session() {
        assert_eq!(typed(&["ko"], "dkssud hello "), "안녕 ㅗ디ㅣㅐ ");
    }

    #[test]
    fn esc_does_nothing_with_one_language() {
        assert_eq!(typed(&["ko"], "dkssud⎋ "), "안녕 ");
    }

    #[test]
    fn esc_goes_to_the_app_with_one_language() {
        let engine = make_engine(None, &Languages::from_ids(&["en"]).unwrap());
        let mut session = InputSession::new(engine, AppPolicies::default(), true);
        let mut field = FakeTextField::new();
        for c in "hell".chars() {
            assert!(session.handle(Key::Letter(c), &mut field));
        }
        // Esc has nothing to switch to: the word goes in as typed and the app gets the key.
        assert!(!session.handle(Key::Escape, &mut field));
        assert_eq!(field.text(), "hell");
        // Right after a commit too (the flip window): the app's Esc, not a no-op of ours.
        assert!(session.handle(Key::Letter('o'), &mut field));
        assert!(!session.handle(Key::Printable(' '), &mut field));
        field.insert_text(" ", None);
        assert!(!session.handle(Key::Escape, &mut field));
        assert_eq!(field.text(), "hello ");
    }

    #[test]
    fn esc_cycles_in_the_users_order() {
        // ko first: the engine shows 안녕, Esc goes to the next language in the list (English).
        assert_eq!(typed(&["ko", "en"], "dkssud⎋ "), "dkssud ");
    }

    #[test]
    fn japanese_on_without_a_converter_does_not_panic() {
        // Space opens the conversion (the built-in kana converter offers the kana itself).
        assert_eq!(typed(&["en", "ko", "ja"], "kyou "), "きょう");
    }
}
