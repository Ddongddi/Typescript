//! What the user keeps on disk under `%APPDATA%\tacit`: per-app policies, settings, word history.

pub mod history;
pub mod keys;
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

/// Reads the JSON file at `path`: the default when there is none. A file that cannot be read
/// as `T` (hand-edited, or written by another version) is moved aside to `<name>.unreadable`
/// rather than left to be overwritten by the next save, and the default is used.
pub fn load_or_quarantine<T: serde::de::DeserializeOwned + Default>(
    path: Option<&std::path::Path>,
) -> T {
    let Some(path) = path else {
        return T::default();
    };
    let Ok(data) = std::fs::read(path) else {
        return T::default();
    };
    if let Ok(value) = serde_json::from_slice(&data) {
        return value;
    }
    let mut aside = path.as_os_str().to_owned();
    aside.push(".unreadable");
    let aside = std::path::PathBuf::from(aside);
    let _ = std::fs::remove_file(&aside);
    let _ = std::fs::rename(path, &aside);
    T::default()
}

/// Saves `value` as JSON at `path` (atomically). Callers may ignore the result: failing to save
/// (read-only profile, sandboxed app) only costs the learning.
pub fn persist<T: serde::Serialize>(path: &std::path::Path, value: &T) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    write_atomic(path, &json)
}

#[cfg(test)]
mod tests {
    use super::{load_or_quarantine, persist, write_atomic};

    #[test]
    fn a_file_that_cannot_be_read_is_kept_aside_not_overwritten() {
        let dir = std::env::temp_dir().join(format!("ts-quarantine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("words.json");
        let read = |p: &std::path::Path| load_or_quarantine::<Vec<u32>>(Some(p));
        assert!(read(&path).is_empty()); // missing: nothing to move
        std::fs::write(&path, b"{ not a list").unwrap();
        assert!(read(&path).is_empty());
        assert!(!path.exists());
        assert_eq!(
            std::fs::read(dir.join("words.json.unreadable")).unwrap(),
            b"{ not a list"
        );
        persist(&path, &vec![1u32, 2]).unwrap();
        assert_eq!(read(&path), vec![1, 2]);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
