//! The real dictionary through the typing session: Space opens the conversion, the paths and
//! clause candidates come from IPADIC (decide/kanji.rs), and committing a candidate is learned.

use std::sync::Arc;

use tacit_core::session::testing::FakeTextField;
use tacit_core::{
    make_engine, AppPolicies, DictionaryConverter, InputSession, JapaneseConverter, Key, Languages,
    TextClient,
};

fn rig(name: &str) -> (InputSession, FakeTextField, Arc<DictionaryConverter>) {
    let lexicon =
        std::env::temp_dir().join(format!("tacit-kanji-{name}-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&lexicon);
    let engine = make_engine(Some(lexicon), &Languages::legacy(true));
    let converter = Arc::new(DictionaryConverter::new(None));
    let session = InputSession::new(engine, AppPolicies::default(), true)
        .with_japanese(converter.clone(), false);
    (session, FakeTextField::new(), converter)
}

fn type_keys(session: &mut InputSession, field: &mut FakeTextField, script: &str) {
    for ch in script.chars() {
        let handled = if ch.is_ascii_alphabetic() {
            session.handle(Key::Letter(ch), field)
        } else {
            session.handle(Key::Printable(ch), field)
        };
        if !handled && !ch.is_ascii_alphabetic() {
            field.insert_text(&ch.to_string(), None);
        }
    }
}

#[test]
fn space_converts_with_the_dictionary() {
    let (mut session, mut field, _) = rig("space");
    type_keys(&mut session, &mut field, "kyouhaiitenkidesune");
    assert_eq!(field.text(), "きょうはいいてんきですね"); // the preview is kana
    assert!(session.handle(Key::Printable(' '), &mut field));
    {
        let conversion = session.conversion().expect("conversion open");
        let texts: Vec<&str> = conversion
            .candidates
            .iter()
            .take(3)
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(texts[0], "今日はいい天気ですね", "{texts:?}");
    }
    assert!(session.handle(Key::Enter, &mut field));
    assert_eq!(field.text(), "今日はいい天気ですね");
}

#[test]
fn a_clause_candidate_keeps_converting_the_rest() {
    let (mut session, mut field, _) = rig("clause");
    type_keys(&mut session, &mut field, "kyouhaiitenkidesune");
    session.handle(Key::Printable(' '), &mut field);
    // ← to the first clause (きょうは: four kana).
    for _ in 0..8 {
        session.handle(Key::Left, &mut field);
    }
    let conversion = session.conversion().expect("still converting");
    assert_eq!(conversion.current().kana_count, 4);
    assert!(session.handle(Key::Enter, &mut field));
    // The first clause went in; the rest keeps converting.
    let rest = session.conversion().expect("rest continues");
    assert_eq!(rest.kana, "いいてんきですね");
    session.handle(Key::Enter, &mut field);
    assert!(field.text().ends_with("いい天気ですね"), "{}", field.text());
}

#[test]
fn a_chosen_candidate_is_first_next_time() {
    let (mut session, mut field, converter) = rig("learn");
    type_keys(&mut session, &mut field, "kyouhagoogledekensaku");
    session.handle(Key::Printable(' '), &mut field);
    let whole = "きょうはgoogleでけんさく".chars().count();
    let index = {
        let conversion = session.conversion().expect("conversion open");
        conversion
            .candidates
            .iter()
            .position(|c| c.kana_count == whole && c.text != conversion.candidates[0].text)
            .expect("more than one whole-reading candidate")
    };
    for _ in 0..index {
        session.handle(Key::Printable(' '), &mut field);
    }
    let chosen = session.conversion().unwrap().current().text.clone();
    session.handle(Key::Enter, &mut field);
    // Committing a candidate is remembered for the reading it covered.
    let again = converter.candidates("きょうはgoogleでけんさく");
    assert_eq!(again[0].text, chosen);
}

#[test]
fn a_reading_remembers_its_five_latest_picks_newest_first_and_saves_them() {
    use tacit_core::ConversionCandidate;
    let path = std::env::temp_dir().join(format!("tacit-kanji-cap-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let converter = DictionaryConverter::new(Some(path.clone()));
    for pick in ["一", "二", "三", "四", "五", "六"] {
        converter.learn(&ConversionCandidate::new(pick, 2), "いち");
    }
    let saved: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["いち"], ["六", "五", "四", "三", "二"]); // 一 was pushed out
    let _ = std::fs::remove_file(&path);
}

#[test]
fn separate_converters_keep_each_others_learned_choices() {
    use tacit_core::ConversionCandidate;
    let path = std::env::temp_dir().join(format!("tacit-kanji-shared-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let first = DictionaryConverter::new(Some(path.clone()));
    let second = DictionaryConverter::new(Some(path.clone()));
    first.learn(&ConversionCandidate::new("返還", 4), "へんかん");
    second.learn(&ConversionCandidate::new("二本", 3), "にほん");
    first.reload();
    assert_eq!(first.candidates("にほん")[0].text, "二本");
    let reloaded = DictionaryConverter::new(Some(path.clone()));
    assert_eq!(reloaded.candidates("へんかん")[0].text, "返還");
    assert_eq!(reloaded.candidates("にほん")[0].text, "二本");
    let _ = std::fs::remove_file(path);
}
