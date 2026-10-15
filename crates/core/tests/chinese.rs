//! The Chinese (Pinyin) track: engine decisions over the zh dictionary and the session
//! flow for a session with English, Korean and Chinese on. The dictionary is the real
//! packed table (the tests build with the embedded packs), so these lock the shipped
//! behavior: nihao becomes 你好, and English words that split into syllables (women,
//! shanghai, banana) stay English until the sentence is already Chinese.

use typescript_core::session::testing::FakeTextField;
use typescript_core::{
    make_engine, AppPolicies, Decision, DecisionContext, DecisionEngine, InputSession, Key,
    Language, Languages,
};

fn langs() -> Languages {
    Languages::from_ids(&["en", "ko", "zh"]).unwrap()
}

fn decide_with(previous: Option<Language>, keys: &str) -> Decision {
    let engine = make_engine(None, &langs());
    engine.decide(
        keys,
        &DecisionContext {
            previous_language: previous,
            partial: false,
        },
    )
}

fn decide(keys: &str) -> Decision {
    decide_with(None, keys)
}

#[test]
fn dictionary_words_convert() {
    for (keys, want) in [
        ("nihao", "你好"),
        ("nihaoma", "你好吗"),
        ("xiexie", "谢谢"),
        ("zhongguo", "中国"),
        ("woshi", "我是"),
        ("xuexi", "学习"),
        ("gongzuo", "工作"),
        ("shijie", "世界"),
        ("duibuqi", "对不起"),
        ("zaijian", "再见"),
        ("pengyou", "朋友"),
    ] {
        let d = decide(keys);
        assert_eq!(d.language, Language::Chinese, "{keys}: {}", d.explanation);
        assert_eq!(d.output, want, "{keys}: {}", d.explanation);
    }
}

#[test]
fn english_words_stay_english() {
    // These split into pinyin syllables; the dictionary keeps them English without
    // Chinese context (the engine's English-word protection).
    for keys in ["hello", "banana", "the", "man", "women", "shanghai", "blog"] {
        let d = decide(keys);
        assert_eq!(d.language, Language::English, "{keys}: {}", d.explanation);
        assert_eq!(d.output, keys);
    }
}

#[test]
fn chinese_context_picks_chinese() {
    let d = decide_with(Some(Language::Chinese), "women");
    assert_eq!(d.language, Language::Chinese, "{}", d.explanation);
    assert_eq!(d.output, "我们");
    let d = decide_with(Some(Language::Chinese), "shanghai");
    assert_eq!(d.language, Language::Chinese, "{}", d.explanation);
    assert_eq!(d.output, "上海");
    let d = decide_with(Some(Language::Chinese), "xian");
    assert_eq!(d.language, Language::Chinese, "{}", d.explanation);
    // A single syllable stays the reading, not a bare character.
    assert_eq!(d.output, "xian");
    // A real English word after Chinese is still English.
    let d = decide_with(Some(Language::Chinese), "hello");
    assert_eq!(d.language, Language::English, "{}", d.explanation);
}

#[test]
fn chinese_off_means_no_chinese() {
    let engine = make_engine(None, &Languages::from_ids(&["en", "ko"]).unwrap());
    let d = engine.decide("nihao", &DecisionContext::default());
    assert_ne!(d.language, Language::Chinese);
    assert_eq!(d.output, "nihao");
}

#[test]
fn a_single_syllable_stays_the_reading() {
    let d = decide("ni");
    assert_eq!(d.language, Language::Chinese, "{}", d.explanation);
    assert_eq!(d.output, "ni");
}

struct Rig {
    session: InputSession,
    field: FakeTextField,
}

impl Rig {
    fn new() -> Self {
        let engine = make_engine(None, &langs());
        Self {
            session: InputSession::new(engine, AppPolicies::default(), true),
            field: FakeTextField::new(),
        }
    }

    fn type_keys(&mut self, keys: &str) {
        for ch in keys.chars() {
            self.session.handle(Key::Letter(ch), &mut self.field);
        }
    }
}

#[test]
fn typing_pinyin_shows_and_commits_the_word() {
    let mut rig = Rig::new();
    rig.type_keys("nihao");
    // The underlined preview is already the word.
    assert_eq!(rig.field.text(), "你好");
    assert!(rig.session.is_composing());
    let handled = rig.session.handle(Key::Printable(' '), &mut rig.field);
    assert!(!handled); // only the word goes in; the app types the space itself
    assert_eq!(rig.field.text(), "你好");
    assert!(!rig.session.is_composing());
    // The sentence continues in Chinese right after.
    rig.type_keys("zhongguo");
    assert_eq!(rig.field.text(), "你好中国");
    rig.session.handle(Key::Printable(' '), &mut rig.field);
    assert_eq!(rig.field.text(), "你好中国");
}

#[test]
fn esc_cycles_chinese_and_english() {
    let mut rig = Rig::new();
    rig.type_keys("nihao");
    assert_eq!(rig.field.text(), "你好");
    // The Korean reading of these keys is broken Hangul, so Esc visits English and
    // Chinese only.
    assert!(rig.session.handle(Key::Cycle, &mut rig.field));
    assert_eq!(rig.field.text(), "nihao");
    assert!(rig.session.handle(Key::Cycle, &mut rig.field));
    assert_eq!(rig.field.text(), "你好");
}

#[test]
fn delete_walks_back_the_reading() {
    let mut rig = Rig::new();
    rig.type_keys("nihao");
    assert_eq!(rig.field.text(), "你好");
    assert!(rig.session.handle(Key::Delete, &mut rig.field));
    assert_eq!(rig.session.keys(), "niha");
    assert_eq!(rig.field.text(), "ni ha");
    assert!(rig.session.handle(Key::Enter, &mut rig.field));
    assert_eq!(rig.field.text(), "ni ha");
}
