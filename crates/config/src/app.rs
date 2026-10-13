//! The dashboard's brain: a request in, a response out. No sockets here, so every rule (token,
//! Host, JSON only, size, validation) is tested directly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use typescript_core::store::policy::Override;
use typescript_core::{pack, Language, Settings};

use crate::env::Env;
use crate::fetch::Fetch;
use crate::languages;
use crate::source::Source;
use crate::update;

pub const MAX_BODY: usize = 256 * 1024;
const PAGE: &str = include_str!("dashboard.html");
/// The settings the page may change, and how each is checked.
const BOOL_SETTINGS: [&str; 6] = [
    "returnConfirmsOnly",
    "autocomplete",
    "liveConversion",
    "fullWidthDigits",
    "fullWidthSpace",
    "checkUpdates",
];

pub struct Request {
    pub method: String,
    /// The path and query as sent.
    pub path: String,
    pub host: Option<String>,
    /// The `X-Token` header.
    pub token: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

fn json_response(status: u16, value: &Value) -> Response {
    Response {
        status,
        content_type: "application/json; charset=utf-8",
        body: value.to_string().into_bytes(),
    }
}

fn fail(status: u16, message: &str) -> Response {
    json_response(status, &json!({ "error": message }))
}

/// Compares without stopping at the first difference.
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// How the dashboard finds out about releases and starts the installer.
/// Starts the downloaded installer script.
pub type Launcher = Box<dyn Fn(&std::path::Path) -> crate::error::Result<()> + Send + Sync>;

pub struct Updates {
    pub api: Box<dyn Fetch>,
    pub url: String,
    pub launch: Launcher,
}

impl Updates {
    pub fn github() -> Updates {
        Updates {
            api: Box::new(crate::fetch::Http),
            url: update::LATEST_API.to_string(),
            launch: Box::new(update::launch_powershell),
        }
    }
}

pub struct App {
    env: Env,
    source: Source,
    token: String,
    port: u16,
    updates: Updates,
    last_seen: Mutex<Instant>,
    quit: AtomicBool,
    /// One change at a time: two downloads at once would race on the same folders.
    busy: Mutex<()>,
}

impl App {
    pub fn new(env: Env, source: Source, token: String, port: u16) -> App {
        App {
            env,
            source,
            token,
            port,
            updates: Updates::github(),
            last_seen: Mutex::new(Instant::now()),
            quit: AtomicBool::new(false),
            busy: Mutex::new(()),
        }
    }

    /// Where releases are looked up and how the installer is started (tests replace both).
    pub fn with_updates(mut self, updates: Updates) -> App {
        self.updates = updates;
        self
    }

    pub fn idle_for(&self) -> Duration {
        self.last_seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .elapsed()
    }

    pub fn should_quit(&self) -> bool {
        self.quit.load(Ordering::SeqCst)
    }

    fn touch(&self) {
        *self.last_seen.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    fn host_ok(&self, host: Option<&str>) -> bool {
        let Some(host) = host else { return false };
        host == format!("127.0.0.1:{}", self.port) || host == format!("localhost:{}", self.port)
    }

    pub fn handle(&self, req: &Request) -> Response {
        // A page from another site can send requests here under a name it controls; the
        // Host header is the name it used.
        if !self.host_ok(req.host.as_deref()) {
            return fail(403, "wrong host");
        }
        let (path, query) = req.path.split_once('?').unwrap_or((req.path.as_str(), ""));
        let token = if path == "/" {
            query.split('&').find_map(|kv| kv.strip_prefix("t="))
        } else {
            req.token.as_deref()
        };
        if !token.is_some_and(|t| same_token(t, &self.token)) {
            return fail(401, "missing or wrong token");
        }
        self.touch();
        match (req.method.as_str(), path) {
            ("GET", "/") => Response {
                status: 200,
                content_type: "text/html; charset=utf-8",
                body: PAGE.as_bytes().to_vec(),
            },
            ("GET", "/api/state") => json_response(200, &self.state()),
            ("GET", "/api/update") => match self.update_info() {
                Ok(v) => json_response(200, &v),
                Err(message) => fail(502, &message),
            },
            ("GET", "/api/apps") => json_response(200, &self.read_object(&self.env.apps)),
            ("POST", "/api/heartbeat") => json_response(200, &json!({})),
            ("POST", "/api/quit") => {
                self.quit.store(true, Ordering::SeqCst);
                json_response(200, &json!({}))
            }
            ("POST", "/api/languages" | "/api/settings" | "/api/apps" | "/api/update") => {
                self.post(path, req)
            }
            (_, "/" | "/api/state" | "/api/apps" | "/api/heartbeat" | "/api/quit")
            | (_, "/api/languages" | "/api/settings" | "/api/update") => {
                fail(405, "method not allowed")
            }
            _ => fail(404, "not found"),
        }
    }

    fn post(&self, path: &str, req: &Request) -> Response {
        if req.body.len() > MAX_BODY {
            return fail(413, "request too large");
        }
        let json_type = req
            .content_type
            .as_deref()
            .is_some_and(|t| t.to_ascii_lowercase().starts_with("application/json"));
        if !json_type {
            return fail(415, "send application/json");
        }
        let Ok(body) = serde_json::from_slice::<Value>(&req.body) else {
            return fail(400, "the body is not valid JSON");
        };
        let _one_at_a_time = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        if path == "/api/update" {
            if body["action"] != "apply" {
                return fail(400, "action has to be apply");
            }
            return match self.apply_update() {
                Ok(message) => json_response(200, &json!({ "message": message })),
                Err(message) => fail(400, &message),
            };
        }
        let done = match path {
            "/api/languages" => self.change_languages(&body),
            "/api/settings" => self.change_settings(&body),
            _ => self.change_apps(&body),
        };
        match done {
            Ok(()) => json_response(200, &self.state()),
            Err(message) => fail(400, &message),
        }
    }

    fn update_info(&self) -> Result<Value, String> {
        let info = update::check(
            &self.env,
            self.updates.api.as_ref(),
            &self.updates.url,
            update::now(),
            true,
        )
        .map_err(|e| e.to_string())?;
        let outdated = self
            .source
            .catalog()
            .map(|c| update::outdated_packs(&self.env, &c))
            .unwrap_or_default();
        Ok(json!({
            "available": info.available || !outdated.is_empty(),
            "latest": info.latest,
            "notes": info.notes,
            "coreChanged": info.available,
            "packsOutdated": outdated,
        }))
    }

    fn apply_update(&self) -> Result<String, String> {
        let launch = self.updates.launch.as_ref();
        update::apply(
            &self.env,
            &self.source,
            self.updates.api.as_ref(),
            &self.updates.url,
            launch,
        )
        .map_err(|e| e.to_string())
    }

    fn change_languages(&self, body: &Value) -> Result<(), String> {
        let id = body["id"].as_str().unwrap_or("");
        match body["action"].as_str() {
            Some("add") => {
                let catalog = self.source.catalog().map_err(|e| e.to_string())?;
                let fetch = self.source.fetch.as_ref();
                languages::add_language(&self.env, fetch, &self.source.base, &catalog, id)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Some("remove") => languages::remove_language(&self.env, id)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Some("order") => {
                let ids: Vec<&str> = body["ids"]
                    .as_array()
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                languages::reorder(&self.env, &ids)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            _ => Err("action has to be add, remove or order".into()),
        }
    }

    fn change_settings(&self, body: &Value) -> Result<(), String> {
        let Some(changes) = body.as_object() else {
            return Err("settings have to be an object".into());
        };
        for (key, value) in changes {
            let ok = match key.as_str() {
                "pauseKey" => matches!(value.as_str(), Some("hangul" | "off")),
                k if BOOL_SETTINGS.contains(&k) => value.is_boolean(),
                _ => return Err(format!("'{key}' is not a setting you can change here")),
            };
            if !ok {
                return Err(format!("'{key}' has a value this setting does not take"));
            }
        }
        let mut file = self.read_object(&self.env.settings);
        let object = file
            .as_object_mut()
            .ok_or("settings.json is not an object")?;
        for (key, value) in changes {
            object.insert(key.clone(), value.clone());
        }
        self.write_json(&self.env.settings, &file)
    }

    fn change_apps(&self, body: &Value) -> Result<(), String> {
        if !body.is_object() {
            return Err("the app rules have to be an object".into());
        }
        let checked: Result<std::collections::BTreeMap<String, Override>, _> =
            serde_json::from_value(body.clone());
        if let Err(e) = checked {
            return Err(format!("an app rule is not valid: {e}"));
        }
        self.write_json(&self.env.apps, body)
    }

    /// The file as a JSON object (BOM or not); empty when missing or broken.
    fn read_object(&self, path: &std::path::Path) -> Value {
        let bytes = std::fs::read(path).unwrap_or_default();
        let bytes = bytes
            .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
            .unwrap_or(&bytes);
        match serde_json::from_slice::<Value>(bytes) {
            Ok(v) if v.is_object() => v,
            _ => Value::Object(Map::new()),
        }
    }

    fn write_json(&self, path: &std::path::Path, value: &Value) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("could not save: {e}"))
    }

    fn state(&self) -> Value {
        let installed = pack::scan(&self.env.packs);
        let on = languages::enabled(&self.env);
        let catalog = self.source.catalog();
        let mut ids: Vec<String> = installed
            .packs
            .iter()
            .map(|f| f.manifest.id.clone())
            .collect();
        if let Ok(c) = &catalog {
            ids.extend(c.packs.iter().map(|p| p.id.clone()));
        }
        ids.sort();
        ids.dedup();
        let mut rows = Vec::new();
        for id in ids {
            let Some(language) = Language::from_id(&id) else {
                continue;
            };
            let have = installed.packs.iter().find(|f| f.manifest.id == id);
            let listed = catalog.as_ref().ok().and_then(|c| c.pack(&id));
            rows.push(json!({
                "id": id,
                "name": have.map(|f| f.manifest.name.clone())
                    .or_else(|| listed.map(|p| p.name.clone())).unwrap_or_default(),
                "installed": have.is_some(),
                "version": have.map(|f| f.manifest.version.clone())
                    .or_else(|| listed.map(|p| p.version.clone())),
                "size": listed.map_or(0, |p| p.size),
                "on": on.contains(language),
                "requiredBy": crate::install::required_by(&self.env, &id),
            }));
        }
        let loaded = Settings::load(Some(&self.env.settings));
        let raw = self.read_object(&self.env.settings);
        json!({
            "languages": rows,
            "order": on.ids(),
            "problems": installed.problems.iter()
                .map(|(dir, why)| format!("{}: {why}", dir.display())).collect::<Vec<_>>(),
            "catalogError": catalog.as_ref().err().map(|e| e.to_string()),
            "settings": {
                "returnConfirmsOnly": loaded.return_confirms_only,
                "pauseKey": loaded.pause_key,
                "autocomplete": loaded.autocomplete,
                "liveConversion": loaded.live_conversion,
                "fullWidthDigits": loaded.full_width_digits,
                "fullWidthSpace": loaded.full_width_space,
                "checkUpdates": raw["checkUpdates"].as_bool().unwrap_or(true),
            },
            "apps": self.read_object(&self.env.apps),
            "version": crate::catalog::CORE_VERSION,
        })
    }
}

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
        assert!(json_of(&last)["error"]
            .as_str()
            .unwrap()
            .contains("at least one"));
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
        assert_eq!(
            app.handle(&req("POST", "/api/settings", Some(good))).status,
            200
        );
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
        assert_eq!(
            app.handle(&req("POST", "/api/apps", Some(rules.clone())))
                .status,
            200
        );
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
        assert_eq!(
            app.handle(&req("POST", "/api/quit", Some(json!({}))))
                .status,
            200
        );
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

    #[test]
    fn a_newer_release_is_reported_and_can_be_applied() {
        use std::sync::{Arc, Mutex};
        let f = fixture("app-update");
        let release = f.root.join("release");
        let zip = crate::testutil::zip_of(&[("install.ps1", b"# installer")]);
        std::fs::write(release.join("typescript-windows.zip"), &zip).unwrap();
        let sum = format!("{}  typescript-windows.zip", crate::hash::sha256_hex(&zip));
        std::fs::write(release.join("typescript-windows.zip.sha256"), sum).unwrap();
        let latest = json!({
            "tag_name": "v9.0.0", "body": "big news",
            "assets": [
                {"name": "typescript-windows.zip", "browser_download_url": "https://x.test/typescript-windows.zip"},
                {"name": "typescript-windows.zip.sha256", "browser_download_url": "https://x.test/typescript-windows.zip.sha256"},
            ],
        });
        std::fs::write(release.join("latest"), latest.to_string()).unwrap();
        let started = Arc::new(Mutex::new(None));
        let seen = started.clone();
        let updates = Updates {
            api: Box::new(crate::fetch::Dir(release)),
            url: "https://api.test/latest".into(),
            launch: Box::new(move |script| {
                *seen.lock().unwrap() = Some(script.to_path_buf());
                Ok(())
            }),
        };
        let app = app(&f).with_updates(updates);
        let info = json_of(&app.handle(&req("GET", "/api/update", None)));
        assert_eq!(info["available"], true);
        assert_eq!(info["coreChanged"], true);
        assert_eq!(info["latest"], "9.0.0");
        assert_eq!(info["notes"], "big news");
        let done = app.handle(&req(
            "POST",
            "/api/update",
            Some(json!({"action": "apply"})),
        ));
        assert_eq!(done.status, 200, "{}", String::from_utf8_lossy(&done.body));
        assert!(json_of(&done)["message"]
            .as_str()
            .unwrap()
            .contains("installer"));
        assert!(started.lock().unwrap().as_ref().unwrap().is_file());
        let bad = app.handle(&req(
            "POST",
            "/api/update",
            Some(json!({"action": "explode"})),
        ));
        assert_eq!(bad.status, 400);
    }

    #[test]
    fn a_failing_release_lookup_is_reported_not_fatal() {
        let f = fixture("app-update-down");
        let updates = Updates {
            api: Box::new(crate::fetch::Dir(f.root.join("nowhere"))),
            url: "https://api.test/latest".into(),
            launch: Box::new(|_| Ok(())),
        };
        let app = app(&f).with_updates(updates);
        let r = app.handle(&req("GET", "/api/update", None));
        assert_eq!(r.status, 502);
        assert!(json_of(&r)["error"].is_string());
    }
}
