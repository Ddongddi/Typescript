//! The dashboard's server: tiny_http on 127.0.0.1 only, one thread per request, and a token the
//! page gets in its address. It stops when the page has been silent for a while or asks to quit.

use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use tiny_http::{Header, Response as HttpResponse, Server};

use crate::app::{App, Request, MAX_BODY};
use crate::env::Env;
use crate::error::{err, Error, Result};
use crate::source::Source;

/// How long the dashboard stays up without hearing from its page (which pings every 5 s, but
/// a browser slows a background tab's timers to about once a minute).
pub const IDLE: Duration = Duration::from_secs(180);

/// Requests handled at the same moment, at most.
const MAX_AT_ONCE: usize = 16;

pub struct Running {
    /// What to open in the browser, token included.
    pub url: String,
    pub app: Arc<App>,
    thread: JoinHandle<()>,
}

impl Running {
    /// Waits until the dashboard has shut itself down.
    pub fn wait(self) {
        let _ = self.thread.join();
    }
}

/// 128 random bits from the OS, as hex.
pub fn new_token() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| Error(format!("no random numbers: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("ASCII header")
}

fn answer(app: &App, mut http: tiny_http::Request) {
    let mut request = Request {
        method: http.method().as_str().to_string(),
        path: http.url().to_string(),
        host: None,
        token: None,
        content_type: None,
        body: Vec::new(),
    };
    for h in http.headers() {
        let name = h.field.as_str().as_str();
        let value = h.value.as_str().to_string();
        if name.eq_ignore_ascii_case("host") {
            request.host = Some(value);
        } else if name.eq_ignore_ascii_case("x-token") {
            request.token = Some(value);
        } else if name.eq_ignore_ascii_case("content-type") {
            request.content_type = Some(value);
        }
    }
    let _ = http
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut request.body);
    let response = app.handle(&request);
    let mut out = HttpResponse::from_data(response.body)
        .with_status_code(response.status)
        .with_header(header("Content-Type", response.content_type))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("Referrer-Policy", "no-referrer"));
    if response.content_type.starts_with("text/html") {
        out = out.with_header(header(
            "Content-Security-Policy",
            "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; connect-src 'self'",
        ));
    }
    let _ = http.respond(out);
}

/// Starts serving on a free port. `idle`: how long without a request ends it.
pub fn start(env: Env, source: Source, idle: Duration) -> Result<Running> {
    let server =
        Server::http("127.0.0.1:0").map_err(|e| Error(format!("could not listen: {e}")))?;
    let Some(port) = server.server_addr().to_ip().map(|a| a.port()) else {
        return err("could not find the port");
    };
    let token = new_token()?;
    let url = format!("http://127.0.0.1:{port}/?t={token}");
    let app = Arc::new(App::new(env, source, token, port));
    let shared = app.clone();
    let working = Arc::new(AtomicUsize::new(0));
    let thread = std::thread::spawn(move || {
        while !shared.should_quit() && shared.idle_for() < idle {
            match server.recv_timeout(Duration::from_millis(100)) {
                Ok(Some(http)) => {
                    // A local program that opens connections endlessly gets turned away
                    // instead of taking every thread and byte.
                    if working.fetch_add(1, Ordering::SeqCst) >= MAX_AT_ONCE {
                        working.fetch_sub(1, Ordering::SeqCst);
                        let _ =
                            http.respond(HttpResponse::from_string("busy").with_status_code(503));
                        continue;
                    }
                    let (app, working) = (shared.clone(), working.clone());
                    std::thread::spawn(move || {
                        answer(&app, http);
                        working.fetch_sub(1, Ordering::SeqCst);
                    });
                }
                Ok(None) => {}
                Err(_) => break,
            }
        }
    });
    Ok(Running { url, app, thread })
}

/// Opens `url` in the default browser; false when that did not work.
pub fn open_browser(url: &str) -> bool {
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    command.spawn().is_ok()
}

/// `typescript-config` with no command: serve the dashboard, open it, and wait until it closes.
pub fn run(env: Env, source: Source, open: bool) -> Result<String> {
    let running = start(env, source, IDLE)?;
    println!("대시보드: {}", running.url);
    println!("브라우저 탭을 닫으면 잠시 뒤 끝납니다. (Ctrl+C로도 끝낼 수 있습니다)");
    if open && !open_browser(&running.url) {
        println!("브라우저를 열지 못했습니다. 위 주소를 직접 여세요.");
    }
    running.wait();
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{fixture, publish};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Instant;

    fn get(port: u16, host: &str, path: &str, token: Option<&str>) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let token = token
            .map(|t| format!("X-Token: {t}\r\n"))
            .unwrap_or_default();
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n{token}Connection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        let status = reply.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, reply)
    }

    fn port_of(url: &str) -> u16 {
        url.split("127.0.0.1:")
            .nth(1)
            .unwrap()
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn token_of(url: &str) -> String {
        url.split("?t=").nth(1).unwrap().to_string()
    }

    #[test]
    fn serves_the_page_and_the_api_only_with_the_token_and_the_right_host() {
        let f = fixture("http");
        let running = start(f.env.clone(), publish(&f), Duration::from_secs(5)).unwrap();
        let port = port_of(&running.url);
        let token = token_of(&running.url);
        let me = format!("127.0.0.1:{port}");
        let (status, page) = get(port, &me, &format!("/?t={token}"), None);
        assert_eq!(status, 200);
        assert!(page.contains("Content-Security-Policy"), "{page}");
        assert!(page
            .to_ascii_lowercase()
            .contains("cache-control: no-store"));
        assert_eq!(get(port, &me, "/api/state", Some(&token)).0, 200);
        assert_eq!(get(port, &me, "/api/state", None).0, 401);
        assert_eq!(get(port, "evil.test", "/api/state", Some(&token)).0, 403);
        running.app.handle(&crate::app::Request {
            method: "POST".into(),
            path: "/api/quit".into(),
            host: Some(me),
            token: Some(token),
            content_type: Some("application/json".into()),
            body: b"{}".to_vec(),
        });
        running.wait();
    }

    #[test]
    fn it_stops_by_itself_when_the_page_goes_quiet() {
        let f = fixture("http-idle");
        let started = Instant::now();
        let running = start(f.env.clone(), publish(&f), Duration::from_millis(300)).unwrap();
        running.wait();
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn tokens_are_long_and_never_repeat() {
        let (a, b) = (new_token().unwrap(), new_token().unwrap());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
