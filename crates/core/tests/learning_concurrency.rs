use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tacit_core::{HeuristicEngine, History, Language, LearningEngine};

fn wait_for(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {path:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "subprocess helper for simultaneous_apps_keep_all_learning"]
fn learning_worker() {
    let root = std::path::PathBuf::from(std::env::var_os("TACIT_TEST_LEARNING_ROOT").unwrap());
    let id: u8 = std::env::var("TACIT_TEST_LEARNING_ID")
        .unwrap()
        .parse()
        .unwrap();
    let engine = LearningEngine::new(
        Box::new(HeuristicEngine::new()),
        Some(root.join("lexicon.json")),
    );
    let history = History::new(Some(root.join("user_history.json")));
    std::fs::write(root.join(format!("ready-{id}")), b"").unwrap();
    wait_for(&root.join("start"));
    for n in 0..40u8 {
        let word = format!(
            "word{}{}{}",
            char::from(b'a' + id),
            char::from(b'a' + n / 26),
            char::from(b'a' + n % 26)
        );
        for _ in 0..2 {
            engine.learn(&word, Language::Korean, Some(Language::English));
            history.record(&word);
        }
        history.flush();
    }
}

#[test]
fn simultaneous_apps_keep_all_learning() {
    let root =
        std::env::temp_dir().join(format!("tacit-concurrent-learning-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut children = Vec::new();
    for id in 0..4 {
        children.push(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "learning_worker", "--ignored"])
                .env("TACIT_TEST_LEARNING_ROOT", &root)
                .env("TACIT_TEST_LEARNING_ID", id.to_string())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    for id in 0..4 {
        wait_for(&root.join(format!("ready-{id}")));
    }
    std::fs::write(root.join("start"), b"").unwrap();
    let mut success = true;
    for child in &mut children {
        success &= child.wait().unwrap().success();
    }
    let store: tacit_core::decide::learning::Store =
        serde_json::from_slice(&std::fs::read(root.join("lexicon.json")).unwrap()).unwrap();
    let history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("user_history.json")).unwrap()).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(success, "learning subprocess failed");
    assert_eq!(store.learned.len(), 160, "some app's corrections were lost");
    assert!(store.once.is_empty());
    let words = history["words"].as_object().unwrap();
    assert_eq!(words.len(), 160, "some app's history was lost");
    assert!(words.values().all(|count| count == 2));
}
