//! Candidate pages: the numbered list the candidate window shows under the word being
//! converted. Language-independent on purpose — today only the Japanese conversion fills a
//! page (session/japanese.rs); a language that offers choices fills the same shape later.
//!
//! The number keys pick from the current page (1-9); committing, wrap-around and clause
//! resizing stay in the conversion itself.

use super::InputSession;

/// How many candidates a page holds: what the number keys pick from.
#[cfg(feature = "japanese")]
pub const PAGE_SIZE: usize = 9;

/// One page of the open conversion, for the platform's candidate window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidatePage {
    /// The texts on this page, in order (at most `PAGE_SIZE`).
    pub texts: Vec<String>,
    /// Where the selected candidate sits on this page (0-based).
    pub selected: usize,
    /// This page (0-based) and how many there are.
    pub page: usize,
    pub pages: usize,
}

/// The page `selected` falls on, where it sits there, and how many pages `total` fills.
#[cfg(feature = "japanese")]
pub(super) fn page_of(selected: usize, total: usize) -> (usize, usize, usize) {
    let selected = selected.min(total.saturating_sub(1));
    let pages = total.div_ceil(PAGE_SIZE).max(1);
    let page = selected / PAGE_SIZE;
    (page, selected - page * PAGE_SIZE, pages)
}

impl InputSession {
    /// What the candidate window shows: the open conversion's current page, or None when
    /// there is no list worth showing (no conversion open, or a single candidate).
    #[cfg(feature = "japanese")]
    pub fn candidate_page(&self) -> Option<CandidatePage> {
        let conversion = self.conversion()?;
        let total = conversion.candidates.len();
        if total < 2 {
            return None;
        }
        let (page, selected, pages) = page_of(conversion.selected, total);
        let texts = conversion
            .candidates
            .iter()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|c| c.text.clone())
            .collect();
        Some(CandidatePage {
            texts,
            selected,
            page,
            pages,
        })
    }
}

#[cfg(all(test, feature = "japanese"))]
mod tests {
    use super::*;

    #[test]
    fn pages_follow_the_selection() {
        assert_eq!(page_of(0, 1), (0, 0, 1));
        assert_eq!(page_of(0, 12), (0, 0, 2));
        assert_eq!(page_of(8, 12), (0, 8, 2));
        assert_eq!(page_of(9, 12), (1, 0, 2));
        assert_eq!(page_of(11, 12), (1, 2, 2));
        assert_eq!(page_of(17, 18), (1, 8, 2));
        // A selection past the end (clamped) still lands on the last page.
        assert_eq!(page_of(9, 9), (0, 8, 1));
    }
}

#[cfg(not(feature = "japanese"))]
impl InputSession {
    pub fn candidate_page(&self) -> Option<CandidatePage> {
        None
    }
}
