//! A big sorted text blob searched in place by byte offset: no per-line index, so loading
//! allocates nothing and touches no page (the TIP pays this cost in every application it is
//! loaded into).

pub struct SortedLines {
    text: &'static str,
}

fn first_field(line: &str) -> &str {
    line.split('\t').next().unwrap_or(line)
}

impl SortedLines {
    /// `text` must be sorted by the first tab-separated field, in byte order (build.rs does that).
    pub fn new(text: &'static str) -> Self {
        Self { text }
    }

    /// The line starting at byte `start`, and the offset of the next line.
    fn line_at(&self, start: usize) -> (&'static str, usize) {
        let rest = &self.text[start..];
        match rest.find('\n') {
            Some(e) => (&rest[..e], start + e + 1),
            None => (rest, self.text.len()),
        }
    }

    /// Offset of the first line whose first field is `>= key` (`text.len()` if none).
    fn lower_bound(&self, key: &str) -> usize {
        self.lower_bound_in(0, self.text.len(), key.as_bytes())
    }

    /// The same inside the lines from `lo` to `hi` (both at the start of a line).
    fn lower_bound_in(&self, mut lo: usize, mut hi: usize, key: &[u8]) -> usize {
        let bytes = self.text.as_bytes();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let start = bytes[lo..mid]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(lo, |p| lo + p + 1);
            let (line, next) = self.line_at(start);
            if first_field(line).as_bytes() < key {
                lo = next;
            } else {
                hi = start;
            }
        }
        lo
    }

    /// All the text, as the range of lines `prefix_range` narrows.
    pub fn span(&self) -> (usize, usize) {
        (0, self.text.len())
    }

    /// The lines of `within` (a range of lines) whose first field starts with `prefix`. Walking
    /// a word letter by letter narrows the range each time, so a step costs a search over a
    /// handful of lines instead of the whole list. `prefix` is ASCII, under 32 bytes.
    pub fn prefix_range(&self, within: (usize, usize), prefix: &[u8]) -> (usize, usize) {
        let lo = self.lower_bound_in(within.0, within.1, prefix);
        let mut after = [0u8; 32];
        let after = &mut after[..prefix.len()];
        after.copy_from_slice(prefix);
        if let Some(last) = after.last_mut() {
            *last += 1;
        }
        (lo, self.lower_bound_in(lo, within.1, after))
    }

    /// The line in `within` (a range of lines) whose first field is `key`.
    pub fn get_in(&self, within: (usize, usize), key: &[u8]) -> Option<&'static str> {
        let lo = self.lower_bound_in(within.0, within.1, key);
        self.exact_in((lo, within.1), key)
    }

    /// The first line of `lines` when its first field is `key`.
    pub fn exact_in(&self, lines: (usize, usize), key: &[u8]) -> Option<&'static str> {
        if lines.0 >= lines.1 {
            return None;
        }
        let (line, _) = self.line_at(lines.0);
        (first_field(line).as_bytes() == key).then_some(line)
    }

    /// Identifies the underlying text (address, length), for caches keyed by model.
    pub fn id(&self) -> (usize, usize) {
        (self.text.as_ptr() as usize, self.text.len())
    }

    /// Number of non-empty lines (a scan; only for statistics).
    pub fn len(&self) -> usize {
        self.text.lines().filter(|l| !l.is_empty()).count()
    }

    #[allow(dead_code)] // pairs with len(); used by the tests
    pub fn is_empty(&self) -> bool {
        self.text.lines().all(str::is_empty)
    }

    /// The whole line whose first field is `key`.
    pub fn get(&self, key: &str) -> Option<&'static str> {
        let start = self.lower_bound(key);
        if start >= self.text.len() {
            return None;
        }
        let (line, _) = self.line_at(start);
        (!line.is_empty() && first_field(line) == key).then_some(line)
    }

    /// Every line whose first field starts with `prefix` (they are contiguous in sorted order).
    pub fn with_prefix<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'static str> + 'a {
        let mut pos = self.lower_bound(prefix);
        std::iter::from_fn(move || {
            while pos < self.text.len() {
                let (line, next) = self.line_at(pos);
                pos = next;
                if !line.is_empty() {
                    return first_field(line).starts_with(prefix).then_some(line);
                }
            }
            None
        })
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
    fn lists_lines_by_prefix() {
        let s = SortedLines::new(
            "app\t1
apple\t2
apply\t3
banana\t4
",
        );
        let found: Vec<_> = s.with_prefix("app").collect();
        assert_eq!(found, ["app\t1", "apple\t2", "apply\t3"]);
        assert_eq!(s.with_prefix("appl").count(), 2);
        assert_eq!(s.with_prefix("c").count(), 0);
        assert_eq!(s.with_prefix("").count(), 4);
    }

    #[test]
    fn handles_empty_and_unterminated_text() {
        assert!(SortedLines::new("").is_empty());
        assert!(SortedLines::new("a\nb").contains("b"));
    }
}
