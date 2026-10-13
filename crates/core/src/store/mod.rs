//! What the user keeps on disk under `%APPDATA%\Typescript`: per-app policies, settings, word history.

pub mod history;
pub mod policy;

/// Writes `bytes` to `path` all at once: into a temporary file next to it, then renamed over it.
/// A reader (the text service, in every app) sees the old file or the new one, never half of
/// one, and a crash in the middle leaves the old file.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let tmp = path.with_file_name(name);
    let written = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::write_atomic;

    #[test]
    fn replaces_the_file_whole_and_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("ts-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("settings.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
