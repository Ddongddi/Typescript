//! The dashboard's "try typing" box against the real packs: what it shows is what the text
//! service would commit.
use std::path::PathBuf;

use serde_json::{json, Value};
use typescript_config::app::{App, Request, Response};
use typescript_config::{cli, env::Env, source::Source};

const TOKEN: &str = "tok";
const PORT: u16 = 4466;

fn send(app: &App, path: &str, body: Value) -> Response {
    app.handle(&Request {
        method: "POST".into(),
        path: path.into(),
        host: Some(format!("127.0.0.1:{PORT}")),
        token: Some(TOKEN.into()),
        content_type: Some("application/json".into()),
        body: body.to_string().into_bytes(),
    })
}

fn post(app: &App, path: &str, body: Value) -> Value {
    let r = send(app, path, body);
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(r.status, 200, "{v}");
    v
}

/// A dashboard over real packs with English and Korean on, in a folder of its own.
fn dashboard(name: &str) -> (App, PathBuf) {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/data");
    let root = std::env::temp_dir().join(format!("ts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    typescript_packgen::write_release(&data, &root.join("release")).unwrap();
    let env = Env {
        settings: root.join("home/settings.json"),
        apps: root.join("home/apps.json"),
        packs: root.join("local/packs"),
    };
    let source = Source::from_spec(root.join("release").to_str().unwrap());
    let setup = ["setup", "--languages", "en,ko"].map(String::from);
    cli::run(&setup, &env, &source).unwrap();
    (App::new(env, source, TOKEN.into(), PORT), root)
}

#[test]
fn typing_is_read_as_the_languages_that_are_on() {
    let (app, root) = dashboard("try");

    let r = post(
        &app,
        "/api/try",
        json!({ "text": "dkssudgktpdy hello, world" }),
    );
    let seg = r["segments"].as_array().unwrap();
    let words: Vec<&Value> = seg.iter().filter(|s| s["keys"].is_string()).collect();
    assert_eq!(words.len(), 3, "{r}");
    assert_eq!(words[0]["lang"], "ko");
    assert_eq!(words[0]["out"], "안녕하세요");
    assert_eq!(words[1]["lang"], "en");
    assert_eq!(words[1]["out"], "hello");
    assert!(
        seg.iter().any(|s| s["text"] == ", "),
        "separators are kept: {r}"
    );

    // Every word says what each language that is on would make of it, so the page can show
    // the road not taken.
    let alts = words[0]["alts"].as_array().unwrap();
    let alt = |lang: &str| {
        alts.iter()
            .find(|a| a["lang"] == lang)
            .map(|a| a["out"].clone())
    };
    assert_eq!(alt("ko"), Some(json!("안녕하세요")), "{r}");
    assert_eq!(alt("en"), Some(json!("dkssudgktpdy")), "{r}");
    assert_eq!(alts.len(), 2, "only the languages that are on: {r}");

    // An app that wants a near-certain reading keeps a doubtful word as typed.
    let strict = post(
        &app,
        "/api/try",
        json!({ "text": "rkskek", "threshold": 1.01 }),
    );
    assert_eq!(strict["segments"][0]["out"], "rkskek");
    assert_eq!(strict["segments"][0]["kept"], true);

    let long = send(&app, "/api/try", json!({ "text": "a".repeat(5000) }));
    assert_eq!(long.status, 400, "long text is refused");
    let _ = std::fs::remove_dir_all(&root);
}

/// The chat box of the settings tab: the keys go through the real session with the saved
/// settings, and a key the session does not take is what the "app" (the chat) gets.
#[test]
fn the_chat_runs_the_real_session_with_the_saved_settings() {
    let (app, root) = dashboard("play");
    let play = |keys: &[&str]| post(&app, "/api/play", json!({ "keys": keys }));
    let word = ["d", "k", "s", "s", "u", "d"];
    fn with<'a>(word: &[&'a str], extra: &[&'a str]) -> Vec<&'a str> {
        [word, extra].concat()
    }

    // Typing: the word is underlined (composing) and already read as Korean.
    let typing = play(&word);
    assert_eq!(typing["text"], "안녕", "{typing}");
    assert_eq!(typing["marked"], json!([0, 2]), "{typing}");
    assert_eq!(typing["submitted"], false);

    // "Enter only confirms" (the default): the first Enter confirms, the second reaches the chat.
    let first = play(&with(&word, &["Enter"]));
    assert_eq!(first["text"], "안녕", "{first}");
    assert_eq!(first["marked"], Value::Null, "{first}");
    assert_eq!(first["submitted"], false, "one Enter did not send it");
    let second = play(&with(&word, &["Enter", "Enter"]));
    assert_eq!(second["submitted"], true, "{second}");
    assert_eq!(second["text"], "안녕");

    // Off: one Enter confirms and sends.
    post(
        &app,
        "/api/settings",
        json!({ "returnConfirmsOnly": false }),
    );
    let once = play(&with(&word, &["Enter"]));
    assert_eq!(once["submitted"], true, "{once}");
    assert_eq!(once["text"], "안녕", "{once}");

    // Keys the session does not take are typed by the field; Backspace undoes them.
    let plain = play(&["1", "2", "Backspace"]);
    assert_eq!(plain["text"], "1", "{plain}");

    let long = send(&app, "/api/play", json!({ "keys": vec!["a"; 500] }));
    assert_eq!(long.status, 400, "a draft is a sentence, not a document");
    let bad = send(&app, "/api/play", json!({ "keys": ["F13"] }));
    assert_eq!(bad.status, 400);
    let _ = std::fs::remove_dir_all(&root);
}

/// A language that is on but whose pack is missing (or too old for this program) is left out,
/// as the input method does; it must not take the page down.
#[test]
fn a_language_without_a_usable_pack_is_left_out() {
    let (app, root) = dashboard("nopack");
    std::fs::write(
        root.join("home/settings.json"),
        r#"{"languages": ["en", "ko", "ja"]}"#,
    )
    .unwrap();
    // A build with the Japanese data inside the binary (another crate of the workspace turns
    // that on) always has Japanese; then all there is to check is that nothing panics.
    let built_in = typescript_core::pack::available(typescript_core::Language::Japanese);
    let r = post(&app, "/api/try", json!({ "text": "dkssud" }));
    let alts = r["segments"][0]["alts"].as_array().unwrap();
    assert!(built_in || alts.len() == 2, "no Japanese reading: {r}");
    let chat = post(
        &app,
        "/api/play",
        json!({ "keys": ["d", "k", "s", "s", "u", "d"] }),
    );
    assert_eq!(chat["text"], "안녕", "{chat}");
    let _ = std::fs::remove_dir_all(&root);
}
