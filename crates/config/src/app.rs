//! The dashboard's brain: a request in, a response out. No sockets here, so every rule (token,
//! Host, JSON only, size, validation) is tested directly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tacit_core::session::testing::FakeTextField;
use tacit_core::session::{Range, TextClient, NOT_FOUND};
use tacit_core::store::policy::Override;
use tacit_core::{
    make_engine, pack, AppPolicies, AppPolicy, DecisionContext, DecisionEngine,
    DictionaryConverter, History, InputSession, JapaneseConverter, Key, Language, Languages,
    LearningEngine, Settings,
};

use crate::env::Env;
use crate::fetch::Fetch;
use crate::languages;
use crate::source::Source;
use crate::update;

pub const MAX_BODY: usize = 256 * 1024;
/// The "try typing" box is for a sentence, not a document.
const MAX_TRY_CHARS: usize = 2000;
/// So is the chat box of the settings tab: keys of one draft message.
const MAX_PLAY_KEYS: usize = 400;
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

/// The change log of this build, shown on the update tab.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

fn json_response(status: u16, value: &Value) -> Response {
    Response {
        status,
        content_type: "application/json; charset=utf-8",
        body: value.to_string().into_bytes(),
    }
}

/// The same failure the same way everywhere: 400 the request is wrong, 401/404/405/413/415 the
/// usual, 429 another job is running, 502 the release server failed, 500 ours. `code` is for
/// scripts and the page; `error` is the sentence to show.
fn fail(status: u16, message: &str) -> Response {
    let code = match status {
        400 => "invalid",
        401 => "unauthorized",
        404 => "not_found",
        405 => "method_not_allowed",
        413 => "too_large",
        415 => "unsupported_type",
        429 => "busy",
        502 => "upstream",
        _ => "internal",
    };
    json_response(status, &json!({ "code": code, "error": message }))
}

/// Compares without stopping at the first difference.
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// How the dashboard finds out about releases and starts the installer.
/// Starts the release's installer (see `crate::update::Installer`).
pub type Launcher =
    Box<dyn Fn(&crate::update::Installer) -> crate::error::Result<()> + Send + Sync>;

pub struct Updates {
    pub api: Box<dyn Fetch>,
    pub url: String,
    pub launch: Launcher,
}

impl Updates {
    pub fn github() -> Updates {
        Updates {
            api: Box::new(crate::fetch::Http::default()),
            url: update::LATEST_API.to_string(),
            launch: Box::new(update::launch_installer),
        }
    }
}

/// File names of the programs running now (for picking the app a rule is for), sorted and
/// lowercase. Empty if the list can't be read.
fn running_apps() -> Vec<String> {
    #[cfg(windows)]
    {
        // The programs that have a window first (what a person means by "my apps"); if that
        // finds nothing (a service session, no PowerShell), every process.
        let with_windows = windowed_apps();
        if with_windows.is_empty() {
            tasklist()
        } else {
            with_windows
        }
    }
    #[cfg(not(windows))]
    ps_apps()
}

/// macOS and Linux: every running program's name from `ps` (no way to tell which have windows).
#[cfg(not(windows))]
fn ps_apps() -> Vec<String> {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-axco", "comm"])
        .output()
    else {
        return Vec::new();
    };
    program_names(&String::from_utf8_lossy(&out.stdout))
}

/// Names from `ps -axco comm` output: lowercase, sorted, no duplicates; the header line and
/// kernel threads (`kworker/0:1`) are left out.
#[cfg_attr(windows, allow(dead_code))]
fn program_names(ps: &str) -> Vec<String> {
    let mut names: Vec<String> = ps
        .lines()
        .skip(1)
        .map(|l| l.trim().to_lowercase())
        .filter(|n| !n.is_empty() && !n.contains(['/', ':']))
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(windows)]
fn command_lines(mut command: std::process::Command) -> Vec<String> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let Ok(out) = command.creation_flags(NO_WINDOW).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[cfg(windows)]
fn exe_names(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = names
        .map(|n| n.trim().to_lowercase())
        .map(|n| {
            if n.ends_with(".exe") {
                n
            } else {
                format!("{n}.exe")
            }
        })
        .filter(|n| n.len() > 4)
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(windows)]
fn windowed_apps() -> Vec<String> {
    let mut command = std::process::Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-Process | Where-Object { $_.MainWindowTitle } | ForEach-Object { $_.ProcessName }",
    ]);
    exe_names(command_lines(command).into_iter())
}

#[cfg(windows)]
fn tasklist() -> Vec<String> {
    let mut command = std::process::Command::new("tasklist");
    command.args(["/fo", "csv", "/nh"]);
    let lines = command_lines(command);
    exe_names(
        lines
            .iter()
            .filter_map(|l| Some(l.strip_prefix('"')?.split('"').next()?.to_string())),
    )
}

pub struct App {
    env: Env,
    source: Source,
    token: String,
    port: u16,
    updates: Updates,
    last_seen: Mutex<Instant>,
    started: Instant,
    quit: AtomicBool,
    /// One change at a time: two downloads at once would race on the same folders.
    /// Held for the slow jobs, downloading a language or updating: one at a time, the next one
    /// asked for meanwhile is turned away (429) instead of piling up behind it.
    slow: Mutex<()>,
    /// The engine the "try typing" box reads with, for the languages that were on when it was built.
    engine: Mutex<Option<(Languages, Arc<LearningEngine>)>>,
    /// The kana-to-kanji dictionary of the chat box, loaded when Japanese first needs it.
    converter: Mutex<Option<Arc<dyn JapaneseConverter>>>,
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
            started: Instant::now(),
            quit: AtomicBool::new(false),
            slow: Mutex::new(()),
            engine: Mutex::new(None),
            converter: Mutex::new(None),
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
        let path = req.path.split('?').next().unwrap_or("/");
        // The page is static and holds nothing private, so a reload (which drops the token from
        // the address) still works; everything it asks of the program needs the token.
        let token_ok = path == "/"
            || path == "/welcome"
            || req
                .token
                .as_deref()
                .is_some_and(|t| same_token(t, &self.token));
        if !token_ok {
            return fail(401, "missing or wrong token");
        }
        self.touch();
        let response = match (req.method.as_str(), path) {
            ("GET", "/") => Response {
                status: 200,
                content_type: "text/html; charset=utf-8",
                body: crate::web::page().as_bytes().to_vec(),
            },
            ("GET", "/welcome") => Response {
                status: 200,
                content_type: "text/html; charset=utf-8",
                body: crate::web::welcome_page().as_bytes().to_vec(),
            },
            ("GET", "/api/state") => json_response(200, &self.state()),
            ("GET", "/api/update") => match self.update_info() {
                Ok(v) => json_response(200, &v),
                Err(message) => fail(502, &message),
            },
            ("GET", "/api/changelog") => json_response(200, &json!({ "text": CHANGELOG })),
            ("GET", "/api/releases") => match self.newer_releases() {
                Ok(v) => json_response(200, &v),
                Err(message) => fail(502, &message),
            },
            ("GET", "/api/learning") => json_response(200, &self.learning_summary()),
            ("GET", "/api/health") => json_response(
                200,
                &json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "uptimeSeconds": self.started.elapsed().as_secs(),
                    "idleSeconds": self.idle_for().as_secs(),
                    "slowJobRunning": self.slow.try_lock().is_err(),
                }),
            ),
            ("GET", "/api/processes") => json_response(200, &json!({ "apps": running_apps() })),
            ("GET", "/api/apps") => {
                json_response(200, &tacit_core::store::read_object(&self.env.apps))
            }
            ("POST", "/api/heartbeat") => json_response(200, &crate::ime::status()),
            ("POST", "/api/quit") => {
                self.quit.store(true, Ordering::SeqCst);
                json_response(200, &json!({}))
            }
            (
                "POST",
                "/api/languages" | "/api/settings" | "/api/apps" | "/api/update" | "/api/try"
                | "/api/play" | "/api/learning" | "/api/uninstall",
            ) => self.post(path, req),
            (
                _,
                "/" | "/welcome" | "/api/state" | "/api/apps" | "/api/processes" | "/api/heartbeat"
                | "/api/quit" | "/api/changelog" | "/api/releases" | "/api/health",
            )
            | (
                _,
                "/api/languages" | "/api/settings" | "/api/update" | "/api/try" | "/api/play"
                | "/api/learning" | "/api/uninstall",
            ) => fail(405, "method not allowed"),
            _ => fail(404, "not found"),
        };
        // The answer ends the idle time too: a slow one (asking the system which input method
        // is selected, a download) is not time the page was silent. Only requests that passed the
        // checks above get here, so nobody without the token can keep the program alive.
        self.touch();
        response
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
        if path == "/api/try" || path == "/api/play" {
            let answer = if path == "/api/try" {
                self.try_typing(&body)
            } else {
                self.play(&body)
            };
            return match answer {
                Ok(v) => json_response(200, &v),
                Err(message) => fail(400, &message),
            };
        }
        if path == "/api/learning" {
            return match crate::learning::change(&self.env, &body) {
                Ok(()) => json_response(200, &self.learning_summary()),
                Err(e) => fail(400, &e.to_string()),
            };
        }
        if path == "/api/uninstall" {
            return self.uninstall(&body);
        }
        let slow_job = path == "/api/update" || path == "/api/languages";
        let _slow = if slow_job {
            match self.slow.try_lock() {
                Ok(guard) => Some(guard),
                Err(std::sync::TryLockError::Poisoned(e)) => Some(e.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => {
                    return fail(429, "another download or update is still running");
                }
            }
        } else {
            None
        };
        // Packs or the language set may change below: the box must read with what is there after.
        *self.engine.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if path == "/api/update" {
            if body["action"] != "apply" {
                return fail(400, "action has to be apply");
            }
            self.source.forget(); // a new release may have a new catalog
            return match self.apply_update() {
                Ok(message) => json_response(200, &json!({ "message": message })),
                Err(message) => fail(502, &message),
            };
        }
        let done = match path {
            "/api/languages" => self.change_languages(&body),
            "/api/settings" => {
                let _write = crate::languages::SETTINGS_WRITE
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                crate::settings::change(&self.env, &body)
            }
            _ => {
                let _write = crate::languages::SETTINGS_WRITE
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                self.change_apps(&body)
            }
        };
        match done {
            Ok(()) => json_response(200, &self.state()),
            Err(message) => fail(400, &message),
        }
    }

    /// The languages that are on now and have a usable pack, and the engine for them (packs are
    /// read from this user's folder). A language without its pack is left out, as the input
    /// method does; with none left there is nothing to read with.
    fn engine(&self) -> Result<(Languages, Arc<LearningEngine>), String> {
        pack::refresh(&self.env.packs);
        let on = languages::enabled(&self.env)
            .available()
            .ok_or("no language pack is installed: add a language in the 언어 tab")?;
        let mut slot = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((built_for, engine)) = slot.as_ref() {
            if *built_for == on {
                engine.reload(); // the input method may have learned a word since
                return Ok((on, engine.clone()));
            }
        }
        // The same lexicon.json the input method reads, so what it learned works in the chat too.
        let engine = make_engine(Some(self.env.settings.with_file_name("lexicon.json")), &on);
        *slot = Some((on.clone(), engine.clone()));
        Ok((on, engine))
    }

    /// The learned words, each with what it turns into (needs a language pack to show).
    fn learning_summary(&self) -> Value {
        let engine = self.engine().ok().map(|(_, engine)| engine);
        crate::learning::summary(&self.env, |keys, language| {
            engine.as_ref().map(|e| e.render(keys, language))
        })
    }

    /// Reads `text` as typed on a QWERTY keyboard, word by word, as the input method would:
    /// the languages that are on, the word before as context, and `threshold` (an app's
    /// "convert only when sure" setting) deciding whether a doubtful word stays as typed.
    fn try_typing(&self, body: &Value) -> Result<Value, String> {
        let text = body["text"].as_str().ok_or("text has to be a string")?;
        if text.chars().count() > MAX_TRY_CHARS {
            return Err(format!("try at most {MAX_TRY_CHARS} characters"));
        }
        let threshold = body["threshold"]
            .as_f64()
            .unwrap_or(AppPolicy::STANDARD.conversion_threshold);
        let (on, engine) = self.engine()?;
        let mut segments = Vec::new();
        let mut previous = None;
        let mut rest = text;
        while !rest.is_empty() {
            let split = |c: char| {
                c.is_ascii_alphabetic() != rest.starts_with(|f: char| f.is_ascii_alphabetic())
            };
            let end = rest.find(split).unwrap_or(rest.len());
            let (piece, tail) = rest.split_at(end);
            rest = tail;
            if !piece.starts_with(|c: char| c.is_ascii_alphabetic()) {
                if piece.contains(char::from(10)) {
                    previous = None;
                }
                segments.push(json!({ "text": piece }));
                continue;
            }
            let d = engine.decide(
                piece,
                &DecisionContext {
                    previous_language: previous,
                    partial: false,
                    ..Default::default()
                },
            );
            let kept = d.language != Language::English && d.confidence < threshold;
            let (language, out) = if kept {
                (Language::English, piece.to_string())
            } else {
                (d.language, d.output)
            };
            previous = Some(language.sentence_language());
            // What each language that is on would make of the word, whatever was chosen.
            let alts: Vec<Value> = on
                .iter()
                .map(|l| json!({ "lang": l.id(), "out": engine.render(piece, l) }))
                .collect();
            segments.push(json!({
                "keys": piece, "out": out, "lang": language.id(),
                "confidence": d.confidence, "kept": kept, "why": d.explanation,
                "alts": alts, "runnerUp": d.runner_up.map(Language::id),
            }));
        }
        Ok(json!({ "segments": segments }))
    }

    fn converter(&self) -> Arc<dyn JapaneseConverter> {
        let mut slot = self.converter.lock().unwrap_or_else(|e| e.into_inner());
        slot.get_or_insert_with(|| Arc::new(DictionaryConverter::new(None)))
            .clone()
    }

    /// Plays a draft message of the chat box: `keys` (a letter or symbol as itself, or `Enter`,
    /// `Tab`, `Backspace`, `Escape`, `Cycle`) go through a fresh session built like the input
    /// method's, with the saved settings. A key the session does not take is what the chat gets:
    /// typed as it is, or, for Enter, the message is sent (`submitted`).
    fn play(&self, body: &Value) -> Result<Value, String> {
        let keys = body["keys"].as_array().ok_or("keys has to be a list")?;
        if keys.len() > MAX_PLAY_KEYS {
            return Err(format!("try at most {MAX_PLAY_KEYS} keys at a time"));
        }
        let settings = Settings::load(Some(&self.env.settings));
        let policies = AppPolicies::load(Some(&self.env.apps));
        let (on, engine) = self.engine()?;
        let mut session = InputSession::new(engine, policies, settings.return_confirms_only);
        if settings.autocomplete {
            session = session.with_history(Arc::new(History::new(None)));
        }
        if on.contains(Language::Japanese) {
            session = session.with_japanese(self.converter(), settings.live_conversion);
        }
        session.set_full_width_digits(settings.full_width_digits);
        session.set_full_width_space(settings.full_width_space);

        let mut field = FakeTextField::new();
        let mut submitted = false;
        for (i, key) in keys.iter().enumerate() {
            let name = key.as_str().ok_or("every key is a string")?;
            let key = match name {
                "Enter" => Key::Enter,
                "Tab" => Key::Tab,
                "Backspace" => Key::Delete,
                "Escape" => Key::Escape,
                "Cycle" => Key::Cycle,
                one => match (one.chars().next(), one.chars().count()) {
                    (Some(c), 1) if c.is_ascii_alphabetic() => Key::Letter(c),
                    (Some(c), 1) if !c.is_control() => Key::Printable(c),
                    _ => return Err(format!("unknown key: {one}")),
                },
            };
            if session.handle(key, &mut field) {
                continue;
            }
            match key {
                Key::Enter => submitted = i + 1 == keys.len(),
                Key::Letter(c) | Key::Printable(c) => field.insert_text(&c.to_string(), None),
                Key::Delete => {
                    if let Some(last) = field.text().chars().next_back() {
                        let end = field.text().encode_utf16().count();
                        let n = last.len_utf16();
                        let range = Range {
                            location: end - n,
                            length: n,
                        };
                        field.insert_text("", Some(range));
                    }
                }
                _ => {}
            }
        }
        let marked = field.marked_range();
        Ok(json!({
            "text": field.text(),
            "marked": (marked.location != NOT_FOUND).then(|| json!([marked.location, marked.length])),
            "submitted": submitted,
        }))
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

    /// The releases newer than this build, for the changelog (GitHub's list; none offline).
    fn newer_releases(&self) -> Result<Value, String> {
        let notes = update::newer_notes(self.updates.api.as_ref(), &self.updates.url)
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "releases": notes
                .iter()
                .map(|n| json!({ "version": n.version, "date": n.date, "notes": n.notes }))
                .collect::<Vec<_>>(),
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

    fn change_apps(&self, body: &Value) -> Result<(), String> {
        if !body.is_object() {
            return Err("the app rules have to be an object".into());
        }
        let rules: std::collections::BTreeMap<String, Override> =
            serde_json::from_value(body.clone())
                .map_err(|e| format!("an app rule is not valid: {e}"))?;
        // What is saved is what was understood: unknown fields do not get in.
        let clean = serde_json::to_value(&rules).map_err(|e| e.to_string())?;
        self.write_json(&self.env.apps, &clean)
    }

    fn write_json(&self, path: &std::path::Path, value: &Value) -> Result<(), String> {
        let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        tacit_core::store::write_atomic(path, text.as_bytes())
            .map_err(|e| format!("could not save: {e}"))
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
        let defaults = crate::settings::view(&Settings::default(), true);
        let loaded = Settings::load(Some(&self.env.settings));
        let raw = tacit_core::store::read_object(&self.env.settings);
        let loaded = crate::settings::view(&loaded, raw["checkUpdates"].as_bool().unwrap_or(true));
        json!({
            "languages": rows,
            "order": on.ids(),
            "problems": installed.problems.iter()
                .map(|(dir, why)| format!("{}: {why}", dir.display())).collect::<Vec<_>>(),
            "catalogError": catalog.as_ref().err().map(|e| e.to_string()),
            "settings": loaded,
            "os": std::env::consts::OS,
            "defaults": defaults,
            "apps": tacit_core::store::read_object(&self.env.apps),
            "uninstall": self.uninstall_summary(),
            "version": crate::catalog::CORE_VERSION,
        })
    }

    /// Where the program runs from and whether it is a `TACIT_HOME` test run: what decides
    /// what removing may touch.
    fn uninstall_plan(&self) -> (crate::uninstall::Plan, bool) {
        let test_home = std::env::var_os("TACIT_HOME").is_some();
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        (
            crate::uninstall::plan(&self.env, exe_dir.as_deref(), test_home),
            test_home,
        )
    }

    fn uninstall_summary(&self) -> Value {
        let (plan, test_home) = self.uninstall_plan();
        crate::uninstall::summary(&plan, test_home)
    }

    /// `{"confirm": "delete"}`: removes the data (and, on an installed Windows copy, hands the
    /// program to its installer first). The page asks twice before it sends this.
    fn uninstall(&self, body: &Value) -> Response {
        if body["confirm"] != "delete" {
            return fail(400, "confirm has to be delete");
        }
        let (plan, _) = self.uninstall_plan();
        match crate::uninstall::execute(&plan, crate::uninstall::launch) {
            Ok(outcome) => json_response(200, &json!({ "outcome": outcome.name() })),
            Err(e) => fail(500, &format!("could not start the removal: {e}")),
        }
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
    fn the_page_itself_needs_no_token_so_a_reload_works() {
        let f = fixture("app-page");
        let app = app(&f);
        let mut r = req("GET", "/", None);
        r.token = None;
        let page = app.handle(&r);
        assert_eq!(page.status, 200);
        assert!(page.content_type.starts_with("text/html"));
        assert!(String::from_utf8_lossy(&page.body).contains("tacit"));
        r.host = Some("evil.test".into());
        assert_eq!(app.handle(&r).status, 403, "still only for this host");
        let mut api = req("GET", "/api/state", None);
        api.token = None;
        assert_eq!(app.handle(&api).status, 401);
    }

    #[test]
    fn removing_needs_the_exact_confirmation_and_lists_what_it_would_take() {
        let f = fixture("app-uninstall");
        let app = app(&f);
        for body in [
            json!({}),
            json!({"confirm": true}),
            json!({"confirm": "yes"}),
        ] {
            let r = app.handle(&req("POST", "/api/uninstall", Some(body)));
            assert_eq!(r.status, 400, "{}", String::from_utf8_lossy(&r.body));
        }
        assert_eq!(app.handle(&req("GET", "/api/uninstall", None)).status, 405);
        let s = json_of(&app.handle(&req("GET", "/api/state", None)));
        assert!(s["uninstall"]["targets"].is_array(), "{s}");
        assert!(s["uninstall"]["program"].is_boolean(), "{s}");
        // Nothing the fixture holds was touched by the refused requests.
        assert!(f.env.settings.parent().unwrap().exists());
    }

    #[test]
    fn the_welcome_page_is_served_like_the_dashboard_page() {
        let f = fixture("app-welcome");
        let app = app(&f);
        let mut r = req("GET", "/welcome", None);
        r.token = None;
        let page = app.handle(&r);
        assert_eq!(page.status, 200);
        assert!(page.content_type.starts_with("text/html"));
        assert!(String::from_utf8_lossy(&page.body).contains("progressbar"));
        r.host = Some("evil.test".into());
        assert_eq!(app.handle(&r).status, 403, "still only for this host");
        let mut post = req("POST", "/welcome", Some(json!({})));
        post.token = None;
        assert_eq!(app.handle(&post).status, 405);
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
        assert_eq!(s["settings"]["hangulKey"], "Hangul");
        assert_eq!(s["settings"]["hangulAction"], "flip");
        assert_eq!(s["settings"]["flipKeys"], json!(["Esc", "HangulKey"]));
        assert_eq!(s["settings"]["pauseKeys"], json!(["Alt+HangulKey"]));
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
        let good = json!({"liveConversion": true, "pauseKeys": ["RCtrl"]});
        assert_eq!(
            app.handle(&req("POST", "/api/settings", Some(good))).status,
            200
        );
        let v: Value = serde_json::from_slice(&std::fs::read(&f.env.settings).unwrap()).unwrap();
        assert_eq!(v["liveConversion"], true);
        assert_eq!(v["pauseKeys"], json!(["RCtrl"]));
        assert!(v.get("pauseKey").is_none());
        assert_eq!(v["somethingNew"], 7);
        for bad in [
            json!({"nope": true}),
            json!({"liveConversion": "yes"}),
            json!({"pauseKey": "off"}),
            json!([1]),
        ] {
            let r = app.handle(&req("POST", "/api/settings", Some(bad.clone())));
            assert_eq!(r.status, 400, "{bad}");
        }
    }

    #[test]
    fn keys_are_checked_together_and_saved_in_one_spelling() {
        let f = fixture("app-keys");
        let app = app(&f);
        let post = |body: Value| app.handle(&req("POST", "/api/settings", Some(body)));
        let ok = post(json!({"flipKeys": ["esc", "ctrl+space"]}));
        assert_eq!(ok.status, 200);
        assert_eq!(
            json_of(&ok)["settings"]["flipKeys"],
            json!(["Esc", "Ctrl+Space"])
        );
        let clash = post(json!({"pauseKeys": ["Ctrl+Space"]}));
        assert_eq!(clash.status, 400);
        assert!(json_of(&clash)["error"]
            .as_str()
            .unwrap()
            .contains("language switch"));
        for bad in [
            json!({"flipKeys": ["Win+L"]}),
            json!({"flipKeys": "Esc"}),
            json!({"flipKeys": [1]}),
            json!({"pauseKeys": ["Esc", "Esc"]}),
        ] {
            assert_eq!(post(bad.clone()).status, 400, "{bad}");
        }
        // the Han/Eng key can be another key; the lists follow it
        let moved = post(json!({"hangulKey": "f13", "flipKeys": ["Esc", "HangulKey"]}));
        assert_eq!(moved.status, 200);
        assert_eq!(json_of(&moved)["settings"]["hangulKey"], "F13");
        // the Han/Eng key can send a shortcut
        let send = post(json!({"hangulAction": "ctrl+c"}));
        assert_eq!(send.status, 200);
        assert_eq!(json_of(&send)["settings"]["hangulAction"], "Ctrl+C");
        assert_eq!(post(json!({"hangulAction": "Ctrl+Hangul"})).status, 400);
        assert_eq!(post(json!({"hangulAction": 3})).status, 400);
        assert_eq!(post(json!({"hangulAction": "none"})).status, 200);
        // a modifier key can't take part in Alt+HangulKey: given explicitly, that is refused
        let bad = post(json!({"hangulKey": "RCtrl", "pauseKeys": ["Alt+HangulKey"]}));
        assert_eq!(bad.status, 400);
        // the Han/Eng key alone becomes RAlt (laptops that send it as right Alt): the pause
        // key that can't follow is dropped, the switch key follows
        let ralt = post(json!({"hangulKey": "RAlt"}));
        assert_eq!(ralt.status, 200);
        assert_eq!(json_of(&ralt)["settings"]["hangulKey"], "RAlt");
        assert_eq!(json_of(&ralt)["settings"]["pauseKeys"], json!([]));
        assert_eq!(
            json_of(&ralt)["settings"]["flipKeys"],
            json!(["Esc", "HangulKey"])
        );
        assert_eq!(
            post(json!({"hangulKey": "Hangul", "pauseKeys": ["Alt+HangulKey"]})).status,
            200
        );
        assert!(json_of(&bad)["error"]
            .as_str()
            .unwrap()
            .contains("modifier"));
        let off = post(json!({"pauseKeys": []}));
        assert_eq!(json_of(&off)["settings"]["pauseKeys"], json!([]));
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
        let policies = tacit_core::AppPolicies::load(Some(&f.env.apps));
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
    fn failures_carry_a_code_and_the_health_check_answers() {
        let f = fixture("app-codes");
        let app = app(&f);
        let r = app.handle(&req("GET", "/api/nothing", None));
        assert_eq!(
            (r.status, json_of(&r)["code"].as_str()),
            (404, Some("not_found"))
        );
        let r = app.handle(&req("POST", "/api/settings", Some(json!({ "nope": true }))));
        let body = json_of(&r);
        assert_eq!((r.status, body["code"].as_str()), (400, Some("invalid")));
        assert!(body["error"].is_string());
        let health = json_of(&app.handle(&req("GET", "/api/health", None)));
        assert_eq!(health["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(health["slowJobRunning"], false);
        let held = app.slow.lock().unwrap();
        let held_health = json_of(&app.handle(&req("GET", "/api/health", None)));
        assert_eq!(held_health["slowJobRunning"], true);
        drop(held);
    }

    #[test]
    fn what_was_learned_can_be_listed_and_forgotten_over_the_api() {
        let f = fixture("app-learning");
        let app = app(&f);
        let r = app.handle(&req("GET", "/api/learning", None));
        assert_eq!(json_of(&r)["learnedCount"], 0);
        let r = app.handle(&req(
            "POST",
            "/api/learning",
            Some(json!({ "action": "forget", "keys": "zz" })),
        ));
        assert_eq!(r.status, 400);
        let r = app.handle(&req(
            "POST",
            "/api/learning",
            Some(json!({ "action": "clearHistory" })),
        ));
        assert_eq!(json_of(&r)["historyWords"], 0);
    }

    #[test]
    fn a_second_download_is_turned_away_but_settings_still_save() {
        let f = fixture("app-slow");
        let app = app(&f);
        let _running = app.slow.lock().unwrap();
        let add = json!({ "action": "add", "id": "ja" });
        assert_eq!(
            app.handle(&req("POST", "/api/languages", Some(add))).status,
            429
        );
        let update = json!({ "action": "apply" });
        assert_eq!(
            app.handle(&req("POST", "/api/update", Some(update))).status,
            429
        );
        let set = json!({ "autocomplete": false });
        assert_eq!(
            app.handle(&req("POST", "/api/settings", Some(set))).status,
            200
        );
    }

    #[test]
    fn the_change_log_of_this_build_is_served() {
        let f = fixture("app-changelog");
        let app = app(&f);
        let text = json_of(&app.handle(&req("GET", "/api/changelog", None)))["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.contains(&format!("## [{}]", env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn the_page_stays_alive_while_it_talks_and_quits_on_request() {
        let f = fixture("app-alive");
        let app = app(&f);
        std::thread::sleep(std::time::Duration::from_millis(600));
        let idle = app.idle_for();
        assert!(idle >= std::time::Duration::from_millis(600));
        // The heartbeat asks Windows which input method is selected, which takes seconds on a
        // loaded machine: the idle time restarts when the answer is done, however long it took.
        app.handle(&req("POST", "/api/heartbeat", Some(json!({}))));
        assert!(app.idle_for() < idle);
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
        let zip = crate::testutil::zip_of(&[(crate::update::INSTALLER_SCRIPT, b"# installer")]);
        let name = crate::update::INSTALLER_ZIP;
        std::fs::write(release.join(name), &zip).unwrap();
        let sum = format!("{}  {name}", crate::hash::sha256_hex(&zip));
        std::fs::write(release.join(format!("{name}.sha256")), sum).unwrap();
        let latest = json!({
            "tag_name": "v9.0.0", "body": "big news",
            "assets": [
                {"name": name, "browser_download_url": format!("https://x.test/{name}")},
                {"name": format!("{name}.sha256"), "browser_download_url": format!("https://x.test/{name}.sha256")},
            ],
        });
        std::fs::write(release.join("latest"), latest.to_string()).unwrap();
        let started = Arc::new(Mutex::new(None));
        let seen = started.clone();
        let updates = Updates {
            api: Box::new(crate::fetch::Dir(release)),
            url: "https://api.test/latest".into(),
            launch: Box::new(move |what| {
                if let crate::update::Installer::Script(script) = what {
                    *seen.lock().unwrap() = Some(script.clone());
                }
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
        let message = json_of(&done)["message"].as_str().unwrap().to_string();
        assert!(message.contains("installer"), "{message}");
        assert!(
            message.contains("tacit-config again"),
            "the open settings page is the old version: {message}"
        );
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

    #[test]
    fn only_known_app_rule_fields_are_saved() {
        let f = fixture("app-apps-clean");
        let app = app(&f);
        let rules = json!({"a.exe": {"enabled": false, "surprise": 1}});
        assert_eq!(
            app.handle(&req("POST", "/api/apps", Some(rules))).status,
            200
        );
        let saved: Value = serde_json::from_slice(&std::fs::read(&f.env.apps).unwrap()).unwrap();
        assert_eq!(saved, json!({"a.exe": {"enabled": false}}));
    }

    #[test]
    fn running_apps_come_as_sorted_lowercase_exe_names() {
        let f = fixture("app-processes");
        let app = app(&f);
        let r = app.handle(&req("GET", "/api/processes", None));
        assert_eq!(r.status, 200);
        let names: Vec<String> = json_of(&r)["apps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(names
            .iter()
            .all(|n| (!cfg!(windows) || n.ends_with(".exe")) && *n == n.to_lowercase()));
        assert!(
            names.windows(2).all(|w| w[0] < w[1]),
            "sorted, no duplicates"
        );
        if cfg!(windows) {
            assert!(!names.is_empty());
        }
    }

    #[test]
    fn ps_output_becomes_program_names() {
        let ps = "COMMAND
bash
Google Chrome
kworker/0:1
bash

";
        assert_eq!(program_names(ps), vec!["bash", "google chrome"]);
    }

    #[test]
    fn state_tells_the_defaults_so_the_page_can_mark_changed_settings() {
        let f = fixture("app-defaults");
        let app = app(&f);
        let s = json_of(&app.handle(&req("GET", "/api/state", None)));
        assert_eq!(s["defaults"]["returnConfirmsOnly"], true);
        assert_eq!(s["defaults"]["hangulKey"], "Hangul");
        assert_eq!(s["defaults"]["flipKeys"], json!(["Esc", "HangulKey"]));
        assert_eq!(s["defaults"]["pauseKeys"], json!(["Alt+HangulKey"]));
        assert_eq!(s["os"], std::env::consts::OS);
        assert_eq!(s["defaults"]["checkUpdates"], true);
    }
}
