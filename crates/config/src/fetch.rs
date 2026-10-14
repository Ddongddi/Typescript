//! Where bytes come from: the release on GitHub, or a folder (tests, offline installs).

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{err, Error, Result};

pub trait Fetch: Send + Sync {
    /// Bytes come over the network (so hosts are worth checking), not from a folder.
    fn is_remote(&self) -> bool {
        false
    }

    /// The bytes at `url`, failing if there are more than `max`.
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>>;
}

/// HTTPS downloads that give up: a connection that hangs must not hang the program with it.
pub struct Http {
    /// The whole request, connection to last byte.
    pub total: Duration,
}

impl Default for Http {
    fn default() -> Self {
        Http {
            total: Duration::from_secs(120),
        }
    }
}

impl Http {
    /// For the once-a-day look for news, which must never hold a command up.
    pub fn quick() -> Http {
        Http {
            total: Duration::from_secs(8),
        }
    }
}

impl Fetch for Http {
    fn is_remote(&self) -> bool {
        true
    }

    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>> {
        if !is_secure(url) {
            return err(format!(
                "refusing to download {url}: only https:// is allowed"
            ));
        }
        // A host with several addresses can have one that does not answer on some networks (the
        // system lists it first now and then): connecting to it stalls until the timeout, which
        // is about half of what is allowed. So the first try is impatient, and only when the
        // connection fails is it tried again with the time a slow link may need.
        self.get_within(url, max, CONNECT_QUICK)
            .or_else(|e| {
                if connect_failed(&e) {
                    self.get_within(url, max, CONNECT_PATIENT)
                } else {
                    Err(e)
                }
            })
            .map_err(|e| Error(format!("download of {url} failed: {e}")))
    }
}

/// How long the first try waits to connect, and the second.
const CONNECT_QUICK: Duration = Duration::from_secs(3);
const CONNECT_PATIENT: Duration = Duration::from_secs(15);

impl Http {
    fn get_within(
        &self,
        url: &str,
        max: usize,
        connect: Duration,
    ) -> std::result::Result<Vec<u8>, ureq::Error> {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(connect))
            .timeout_global(Some(self.total))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut response = agent
            .get(url)
            .header("User-Agent", "typescript-config")
            .call()?;
        response
            .body_mut()
            .with_config()
            .limit(max as u64)
            .read_to_vec()
    }
}

/// The connection could not be made (in time), as opposed to a server that answered with an error.
fn connect_failed(e: &ureq::Error) -> bool {
    matches!(
        e,
        ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::Timeout(ureq::Timeout::Connect)
    )
}

/// Downloads must be https, so a hash that comes from the same place cannot vouch for a
/// download an attacker on the network rewrote. This machine itself (a release served for
/// testing) is the one exception.
fn is_secure(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let name = match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => host.split(':').next().unwrap_or(""),
    };
    matches!(name, "localhost" | "127.0.0.1" | "::1")
}

/// A folder standing in for a release: `url` is looked up by its last path segment.
pub struct Dir(pub PathBuf);

impl Fetch for Dir {
    fn get(&self, url: &str, max: usize) -> Result<Vec<u8>> {
        let name = url.rsplit(['/', '\\']).next().unwrap_or(url);
        let bytes = std::fs::read(self.0.join(name)).map_err(|e| Error(format!("{name}: {e}")))?;
        if bytes.len() > max {
            return err(format!("{name} is larger than expected"));
        }
        Ok(bytes)
    }
}

/// `rel` against the location of `base` (a URL or path of a file): an absolute URL stays as is.
pub fn resolve(base: &str, rel: &str) -> String {
    if rel.contains("://") {
        return rel.to_string();
    }
    match base.rfind(['/', '\\']) {
        Some(i) => format!("{}{rel}", &base[..=i]),
        None => rel.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_or_this_machine_may_be_downloaded_from() {
        assert!(is_secure("https://github.com/x/packs.json"));
        assert!(is_secure("http://127.0.0.1:8080/packs.json"));
        assert!(is_secure("http://localhost/packs.json"));
        assert!(is_secure("http://[::1]:9/packs.json"));
        assert!(!is_secure("http://github.com/x"));
        assert!(!is_secure("http://127.0.0.1.evil.test/x"));
        assert!(!is_secure("http://127.0.0.1@evil.test/x"));
        assert!(!is_secure("ftp://x.test/a"));
        assert!(Http::default().get("http://example.test/a", 10).is_err());
    }

    #[test]
    fn only_a_connection_that_could_not_be_made_is_tried_again_with_more_time() {
        assert!(connect_failed(&ureq::Error::HostNotFound));
        assert!(connect_failed(&ureq::Error::ConnectionFailed));
        assert!(connect_failed(&ureq::Error::Timeout(
            ureq::Timeout::Connect
        )));
        // A server that answered (even with an error), or a slow body, is no reason to retry.
        assert!(!connect_failed(&ureq::Error::StatusCode(404)));
        assert!(!connect_failed(&ureq::Error::BodyExceedsLimit(10)));
        assert!(!connect_failed(&ureq::Error::Timeout(
            ureq::Timeout::Global
        )));
    }

    #[test]
    fn a_download_from_this_machine_works() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
                )
                .unwrap();
        });
        let bytes = Http::default()
            .get(&format!("http://127.0.0.1:{port}/packs.json"), 100)
            .unwrap();
        assert_eq!(bytes, b"hello");
        server.join().unwrap();
    }

    #[test]
    fn resolves_against_the_base() {
        assert_eq!(
            resolve("https://x.test/rel/packs.json", "en.zip"),
            "https://x.test/rel/en.zip"
        );
        assert_eq!(
            resolve("https://x.test/a/packs.json", "https://y.test/z.zip"),
            "https://y.test/z.zip"
        );
        assert_eq!(resolve("packs.json", "en.zip"), "en.zip");
    }

    #[test]
    fn a_folder_serves_files_by_name_and_respects_the_cap() {
        let dir = std::env::temp_dir().join(format!("ts-cfg-fetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.bin"), [1u8; 10]).unwrap();
        let source = Dir(dir.clone());
        assert_eq!(
            source.get("https://x.test/any/a.bin", 10).unwrap().len(),
            10
        );
        assert!(source.get("a.bin", 9).is_err());
        assert!(source.get("missing.bin", 100).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
