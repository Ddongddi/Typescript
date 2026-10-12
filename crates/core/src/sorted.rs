//! A big sorted text blob searched in place: one `u32` per line instead of a hash map of
//! owned strings (the TIP pays this cost in every application it is loaded into).

pub struct SortedLines {
    text: &'static str,
    /// Start of each non-empty line.
    starts: Vec<u32>,
}

impl SortedLines {
    /// `text` must be sorted by the first tab-separated field, in byte order (build.rs does that).
    pub fn new(text: &'static str) -> Self {
        let mut starts = Vec::with_capacity(text.len() / 8);
        let mut pos = 0usize;
        for line in text.split_inclusive('\n') {
            if line != "\n" {
                starts.push(pos as u32);
            }
            pos += line.len();
        }
        Self { text, starts }
    }

    fn line(&self, i: usize) -> &'static str {
        let start = self.starts[i] as usize;
        let end = self.text[start..].find('\n').map_or(self.text.len(), |e| start + e);
        &self.text[start..end]
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    #[allow(dead_code)] // pairs with len(); used by the tests
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// The whole line whose first field is `key`.
    pub fn get(&self, key: &str) -> Option<&'static str> {
        let (mut lo, mut hi) = (0usize, self.starts.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            let line = self.line(mid);
            let field = line.split('\t').next().unwrap_or(line);
            match field.cmp(key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(line),
            }
        }
        None
    }

    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_lines_by_first_field() {
        let s = SortedLines::new("apple\t10\nbanana\t20\ncherry\t30\n");
        assert_eq!(s.len(), 3);
        assert_eq!(s.get("banana"), Some("banana\t20"));
        assert_eq!(s.get("cherry"), Some("cherry\t30"));
        assert!(s.contains("apple"));
        assert!(!s.contains("app"));
        assert!(!s.contains("zzz"));
    }

    #[test]
    fn handles_empty_and_unterminated_text() {
        assert!(SortedLines::new("").is_empty());
        assert!(SortedLines::new("a\nb").contains("b"));
    }
}
