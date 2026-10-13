//! The languages the user turned on, in their order: the order Esc cycles through them.

use crate::decide::engine::Language;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Languages(Vec<Language>);

impl Languages {
    /// Duplicates and `Mixed` are dropped; None when nothing is left.
    pub fn new(list: impl IntoIterator<Item = Language>) -> Option<Languages> {
        let mut out: Vec<Language> = Vec::new();
        for l in list {
            if l != Language::Mixed && !out.contains(&l) {
                out.push(l);
            }
        }
        (!out.is_empty()).then_some(Languages(out))
    }

    /// Unknown ids are skipped.
    pub fn from_ids(ids: &[&str]) -> Option<Languages> {
        Self::new(ids.iter().filter_map(|id| Language::from_id(id)))
    }

    /// What the editions before language packs had: English + Korean, plus Japanese.
    pub fn legacy(japanese: bool) -> Languages {
        let mut list = vec![Language::English, Language::Korean];
        if japanese {
            list.push(Language::Japanese);
        }
        Languages(list)
    }

    pub fn contains(&self, language: Language) -> bool {
        self.0.contains(&language)
    }

    /// Whether a decision in `language` may come out: `Mixed` needs English and Korean.
    pub fn allows(&self, language: Language) -> bool {
        match language {
            Language::Mixed => self.contains(Language::English) && self.contains(Language::Korean),
            other => self.contains(other),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Language> + '_ {
        self.0.iter().copied()
    }

    pub fn first(&self) -> Language {
        self.0[0]
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        false // never: `new` refuses an empty list
    }

    pub fn ids(&self) -> Vec<&'static str> {
        self.0.iter().map(|l| l.id()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Language::*;

    #[test]
    fn keeps_order_and_drops_duplicates() {
        let l = Languages::new([Korean, English, Korean]).unwrap();
        assert_eq!(l.iter().collect::<Vec<_>>(), vec![Korean, English]);
        assert_eq!(l.first(), Korean);
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn rejects_empty_and_mixed() {
        assert!(Languages::new([]).is_none());
        assert!(Languages::new([Mixed]).is_none());
        assert_eq!(Languages::new([Mixed, Japanese]).unwrap().ids(), vec!["ja"]);
    }

    #[test]
    fn parses_ids() {
        assert_eq!(
            Languages::from_ids(&["ko", "ja"]).unwrap().ids(),
            vec!["ko", "ja"]
        );
        assert!(Languages::from_ids(&["xx"]).is_none());
        assert!(Languages::from_ids(&["mixed"]).is_none());
        assert_eq!(
            Languages::from_ids(&["xx", "en"]).unwrap().ids(),
            vec!["en"]
        );
    }

    #[test]
    fn legacy_matches_the_old_editions() {
        assert_eq!(Languages::legacy(false).ids(), vec!["en", "ko"]);
        assert_eq!(Languages::legacy(true).ids(), vec!["en", "ko", "ja"]);
    }

    #[test]
    fn mixed_needs_english_and_korean() {
        assert!(Languages::legacy(false).allows(Mixed));
        assert!(!Languages::new([Korean]).unwrap().allows(Mixed));
        assert!(!Languages::new([English, Japanese]).unwrap().allows(Mixed));
        assert!(Languages::new([Korean]).unwrap().allows(Korean));
        assert!(!Languages::new([Korean]).unwrap().allows(English));
    }
}
