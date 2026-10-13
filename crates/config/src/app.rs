#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{fixture, publish, Fixture};
    use serde_json::{json, Value};

    const TOKEN: &str = "tok-123";
    const PORT: u16 = 4455;

    fn app(f: &Fixture) -> App {
        App::new(f.env.clone(), publish(f), TOKEN.into(), PORT)
    }

    fn req(method: &str, path: &str, body: Option<Value>) -> Request {
        Request {
            method: method.into(),
            path: path.into(),
            host: Some(format!("127.0.0.1:{PORT}")),
            token: Some(TOKEN.into()),
            content_type: body.as_ref().map(|_| "application/json".into()),
            body: body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
        }
    }

    fn json_of(r: &Response) -> Value {
        serde_json::from_slice(&r.body)
            .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&r.body)))
    }

    #[test]
    fn requests_without_the_token_are_refused() {
        let f = fixture("app-token");
        let app = app(&f);
        let mut r = req("GET", "/api/state", None);
        r.token = None;
        assert_eq!(app.handle(&r).status, 401);
        r.token = Some("wrong".into());
        assert_eq!(app.handle(&r).status, 401);
        assert_eq!(app.handle(&req("GET", "/api/state", None)).status, 200);
    }

    #[test]
    fn another_site_pointing_a_name_at_localhost_is_refused() {
        let f = fixture("app-host");
        let app = app(&f);
        for host in [
            "evil.test",
            "evil.test:4455",
            "127.0.0.1:9999",
            "127.0.0.1",
            "localhost:1",
        ] {
            let mut r = req("GET", "/api/state", None);
            r.host = Some(host.into());
            assert_eq!(app.handle(&r).status, 403, "{host}");
        }
        let mut r = req("GET", "/api/state", None);
        r.host = None;
        assert_eq!(app.handle(&r).status, 403);
        let mut r = req("GET", "/api/state", None);
        r.host = Some(format!("localhost:{PORT}"));
        assert_eq!(app.handle(&r).status, 200);
    }

    #[test]
    fn the_page_needs_the_token_in_its_address() {
        let f = fixture("app-page");
        let app = app(&f);
        let mut r = req("GET", "/?t=tok-123", None);
        r.token = None;
        let page = app.handle(&r);
        assert_eq!(page.status, 200);
        assert!(page.content_type.starts_with("text/html"));
        assert!(String::from_utf8_lossy(&page.body).contains("Typescript"));
        r.path = "/?t=nope".into();
        assert_eq!(app.handle(&r).status, 401);
        r.path = "/".into();
        assert_eq!(app.handle(&r).status, 401);
    }

    #[test]
    fn state_lists_languages_settings_and_apps() {
        let f = fixture("app-state");
        let app = app(&f);
        let s = json_of(&app.handle(&req("GET", "/api/state", None)));
        let langs = s["languages"].as_array().unwrap();
        assert_eq!(langs.len(), 3);
        let ja = langs.iter().find(|l| l["id"] == "ja").unwrap();
        assert_eq!(ja["installed"], false);
        assert_eq!(ja["on"], false);
        assert_eq!(s["order"], json!(["en", "ko"]));
        assert_eq!(s["settings"]["pauseKey"], "hangul");
        assert!(s["apps"].is_object());
        assert!(s["catalogError"].is_null());
    }

    #[test]
    fn languages_can_be_added_reordered_and_removed() {
        let f = fixture("app-langs");
        let app = app(&f);
        let add = json!({"action": "add", "id": "ja"});
        let r = app.handle(&req("POST", "/api/languages", Some(add)));
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        assert_eq!(json_of(&r)["order"], json!(["en", "ko", "ja"]));
        let order = json!({"action": "order", "ids": ["ja", "en", "ko"]});
        let r = app.handle(&req("POST", "/api/languages", Some(order)));
        assert_eq!(json_of(&r)["order"], json!(["ja", "en", "ko"]));
        let remove = json!({"action": "remove", "id": "ja"});
        let r = app.handle(&req("POST", "/api/languages", Some(remove)));
        assert_eq!(json_of(&r)["order"], json!(["en", "ko"]));
    }

    #[test]
    fn mistakes_come_back_as_messages_not_crashes() {
        let f = fixture("app-mistakes");
        let app = app(&f);
        let post = |body: Value| app.handle(&req("POST", "/api/languages", Some(body)));
        assert_eq!(post(json!({"action": "remove", "id": "en"})).status, 200);
        let last = post(json!({"action": "remove", "id": "ko"}));
        assert_eq!(last.status, 400);
        assert!(json_of(&last)["error"].as_str().unwrap().contains("at least one"));
        assert_eq!(post(json!({"action": "explode"})).status, 400);
        assert_eq!(post(json!({"action": "add", "id": "xx"})).status, 400);
    }

    #[test]
    fn posts_must_be_json_and_not_huge() {
        let f = fixture("app-post");
        let app = app(&f);
        let body = json!({"action": "add", "id": "ja"});
        let mut r = req("POST", "/api/languages", Some(body));
        r.content_type = Some("text/plain".into());
        assert_eq!(app.handle(&r).status, 415);
        r.content_type = Some("application/x-www-form-urlencoded".into());
        assert_eq!(app.handle(&r).status, 415);
        let mut big = req("POST", "/api/settings", Some(json!({})));
        big.body = vec![b' '; 300 * 1024];
        assert_eq!(app.handle(&big).status, 413);
        assert_eq!(app.handle(&req("DELETE", "/api/state", None)).status, 405);
        assert_eq!(app.handle(&req("GET", "/api/nothing", None)).status, 404);
    }

    #[test]
    fn settings_are_validated_and_keep_unknown_keys() {
        let f = fixture("app-settings");
        std::fs::write(&f.env.settings, r#"{ "somethingNew": 7 }"#).unwrap();
        let app = app(&f);
        let good = json!({"liveConversion": true, "pauseKey": "off"});
        assert_eq!(app.handle(&req("POST", "/api/settings", Some(good))).status, 200);
        let v: Value = serde_json::from_slice(&std::fs::read(&f.env.settings).unwrap()).unwrap();
        assert_eq!(v["liveConversion"], true);
        assert_eq!(v["pauseKey"], "off");
        assert_eq!(v["somethingNew"], 7);
        for bad in [
            json!({"nope": true}),
            json!({"liveConversion": "yes"}),
            json!({"pauseKey": "x"}),
            json!([1]),
        ] {
            let r = app.handle(&req("POST", "/api/settings", Some(bad.clone())));
            assert_eq!(r.status, 400, "{bad}");
        }
    }

    #[test]
    fn app_rules_are_validated_and_saved() {
        let f = fixture("app-apps");
        let app = app(&f);
        let rules = json!({
            "*": {"escapeToApp": false},
            "game.exe": {"enabled": false},
            "wt.exe": {"conversionThreshold": 0.97}
        });
        assert_eq!(app.handle(&req("POST", "/api/apps", Some(rules.clone()))).status, 200);
        assert_eq!(json_of(&app.handle(&req("GET", "/api/apps", None))), rules);
        let policies = typescript_core::AppPolicies::load(Some(&f.env.apps));
        assert!(!policies.policy(Some("game.exe")).enabled);
        for bad in [
            json!({"a.exe": {"enabled": "no"}}),
            json!({"a.exe": {"conversionThreshold": "x"}}),
            json!([]),
            json!({"a.exe": 5}),
        ] {
            let r = app.handle(&req("POST", "/api/apps", Some(bad.clone())));
            assert_eq!(r.status, 400, "{bad}");
        }
    }

    #[test]
    fn the_page_stays_alive_while_it_talks_and_quits_on_request() {
        let f = fixture("app-alive");
        let app = app(&f);
        std::thread::sleep(std::time::Duration::from_millis(40));
        assert!(app.idle_for() >= std::time::Duration::from_millis(40));
        app.handle(&req("POST", "/api/heartbeat", Some(json!({}))));
        assert!(app.idle_for() < std::time::Duration::from_millis(40));
        assert!(!app.should_quit());
        assert_eq!(app.handle(&req("POST", "/api/quit", Some(json!({})))).status, 200);
        assert!(app.should_quit());
    }

    #[test]
    fn without_a_catalog_the_installed_languages_still_show() {
        let f = fixture("app-offline");
        let nowhere = crate::source::Source::from_spec(f.root.join("nowhere").to_str().unwrap());
        let app = App::new(f.env.clone(), nowhere, TOKEN.into(), PORT);
        let s = json_of(&app.handle(&req("GET", "/api/state", None)));
        assert!(s["catalogError"].is_string());
        assert_eq!(s["order"], json!(["en", "ko"]));
    }
}
