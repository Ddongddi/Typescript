use tacit_core::{make_engine, DecisionContext, DecisionEngine, Language, Languages};

#[test]
fn supported_languages_match_the_compiled_modules() {
    let ids: Vec<_> = tacit_core::lang::registry::MODULES
        .iter()
        .map(|m| m.language().id())
        .collect();
    assert_eq!(ids.contains(&"ja"), cfg!(feature = "japanese"));
    assert_eq!(Language::Japanese.supported(), cfg!(feature = "japanese"));
}

#[test]
fn enko_never_decides_or_cycles_into_another_language() {
    let languages = Languages::from_ids(&["en", "ko"]).unwrap();
    let engine = make_engine(None, &languages);
    for keys in [
        "nihongo",
        "kyouhaiitenkidesune",
        "githubde",
        "hello",
        "dkssud",
        "githubdp",
    ] {
        // Old context and learned identifiers must not re-enable a disabled language.
        for previous in [None, Some(Language::Japanese)] {
            let decision = engine.decide(
                keys,
                &DecisionContext {
                    previous_language: previous,
                    ..Default::default()
                },
            );
            assert!(languages.allows(decision.language), "{keys}: {decision:?}");
            assert!(decision.runner_up.is_none_or(|l| languages.allows(l)));
            assert_ne!(decision.language.next(keys, &languages), Language::Japanese);
        }
    }
}

#[cfg(not(feature = "japanese"))]
#[test]
fn old_settings_cannot_enable_an_uncompiled_language() {
    assert!(Languages::from_ids(&["ja"]).is_none());
    assert_eq!(Languages::legacy(true).ids(), ["en", "ko"]);
    let path = std::env::temp_dir().join(format!("tacit-isolation-{}.json", std::process::id()));
    std::fs::write(
        &path,
        r#"{"languages":["ja","ko","en"],"japanese":true,"liveConversion":true}"#,
    )
    .unwrap();
    let settings = tacit_core::Settings::load(Some(&path));
    assert_eq!(settings.languages.ids(), ["ko", "en"]);
    std::fs::remove_file(path).unwrap();

    let mut engine = tacit_core::HeuristicEngine::new();
    engine.english_on = false;
    engine.korean_on = false;
    assert_ne!(
        engine
            .decide("nihongo", &DecisionContext::default())
            .language,
        Language::Japanese
    );
}
