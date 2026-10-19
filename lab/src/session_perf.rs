//! Session preview + completion, separately from the decide-only timings.

use std::fmt::Write;
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tacit_core::session::NoopClient;
use tacit_core::{AppPolicies, History, InputSession, Key, Languages};

fn history(size: usize) -> Arc<History> {
    if size == 0 {
        return Arc::new(History::new(None));
    }
    let mut json = String::from("{\"words\":{");
    for i in 0..size {
        let word = match i {
            0 => "programming".to_string(),
            1 => "안녕하세요".to_string(),
            _ if i % 2 == 0 => {
                let n = i / 2;
                [n % 26, n / 26 % 26, n / 676 % 26, n / 17576 % 26]
                    .map(|n| char::from(b'a' + n as u8))
                    .iter()
                    .collect()
            }
            _ => format!(
                "{}가",
                char::from_u32('가' as u32 + (i / 2) as u32).unwrap()
            ),
        };
        if i != 0 {
            json.push(',');
        }
        write!(json, "\"{word}\":{}", if i < 2 { 100 } else { 2 + i % 7 }).unwrap();
    }
    json.push_str("}}");
    let path = std::env::temp_dir().join(format!(
        "tacit-session-perf-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, json).unwrap();
    let history = Arc::new(History::new(Some(path.clone())));
    std::fs::remove_file(path).unwrap();
    assert_eq!(history.word_count(), size);
    history
}

pub fn run(languages: &Languages) {
    let engine = super::make_engine(None, languages);
    let mut metrics = Default::default();
    for size in [0, 1000, 20_000] {
        let history = history(size);
        for prefix in ["p", "pro", "ㄱ", "가", "안녀", "안녕ㅎ"] {
            let mut times = Vec::new();
            for round in 0..110 {
                let start = Instant::now();
                black_box(history.suggest(black_box(prefix)));
                if round >= 10 {
                    times.push(start.elapsed());
                }
            }
            super::latency(&mut metrics, &format!("history.{size}.{prefix}"), times);
        }
        let mut times = Vec::new();
        for round in 0..25 {
            for keys in [
                "programming",
                "githubdp",
                "dkssudgktpdy",
                "dhsmfdmsgoogledptj",
            ] {
                let mut session = InputSession::new(engine.clone(), AppPolicies::default(), false)
                    .with_history(history.clone());
                for ch in keys.chars() {
                    let start = Instant::now();
                    black_box(session.handle(Key::Letter(ch), &mut NoopClient));
                    black_box(session.suggestion());
                    if round >= 5 {
                        times.push(start.elapsed());
                    }
                }
            }
        }
        super::latency(&mut metrics, &format!("session.{size}"), times);
    }
    println!(
        "profile {} (NoopClient; preview + completion; no commit/disk writes)",
        languages.ids().join("+")
    );
    for (name, value) in metrics {
        println!("{name}\t{value:.3}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fixture_has_the_requested_words_and_completes_both_languages() {
        let history = super::history(20_000);
        assert_eq!(history.suggest("pro").as_deref(), Some("programming"));
        assert_eq!(history.suggest("안녀").as_deref(), Some("안녕하세요"));
    }
}
