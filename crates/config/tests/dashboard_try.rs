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

#[test]
fn typing_is_read_as_the_languages_that_are_on() {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/data");
    let root = std::env::temp_dir().join(format!("ts-try-{}", std::process::id()));
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
    let app = App::new(env, source, TOKEN.into(), PORT);

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
