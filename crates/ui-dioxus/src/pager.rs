//! Paging the trip list (US-63): 50 rows to a page, over the rows the screen
//! has already read. Nothing new is fetched, and the heat map still has
//! every matching trip — paging the *query* is deliberately not this.
//!
//! The arithmetic is plain functions so it is unit-tested; [`Pager`] only
//! shows it and moves the page.

use std::ops::Range;

use dioxus::prelude::*;

pub const PAGE_SIZE: usize = 50;

/// How many pages `len` rows make. An empty list is still one (empty) page,
/// so there is always a page to be on.
pub fn page_count(len: usize) -> usize {
    len.div_ceil(PAGE_SIZE).max(1)
}

/// The rows `page` shows, out of `len`. A page past the end — the list
/// shrank under the owner, say after a bulk edit narrowed a tag-filtered
/// list — shows the last page rather than a blank table.
pub fn page_range(page: usize, len: usize) -> Range<usize> {
    let page = page.min(page_count(len) - 1);
    let start = page * PAGE_SIZE;
    start..(start + PAGE_SIZE).min(len)
}

/// Previous/next and where the owner is. Nothing renders for a list that
/// fits on one page.
#[component]
pub fn Pager(page: Signal<usize>, len: usize) -> Element {
    let count = page_count(len);
    if count <= 1 {
        return rsx! {};
    }
    // The page as shown: the signal may still point past a list that shrank.
    let current = page().min(count - 1);
    rsx! {
        nav { class: "pager",
            button {
                r#type: "button",
                disabled: current == 0,
                onclick: move |_| page.set(current - 1),
                "‹ Previous"
            }
            span { "Page {current + 1} of {count}" }
            button {
                r#type: "button",
                disabled: current + 1 == count,
                onclick: move |_| page.set(current + 1),
                "Next ›"
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn fifty_rows_make_a_page() {
        assert_eq!(page_count(0), 1);
        assert_eq!(page_count(50), 1);
        assert_eq!(page_count(51), 2);
        assert_eq!(page_count(247), 5);
    }

    #[test]
    fn a_page_holds_its_fifty_rows_and_the_last_one_the_rest() {
        assert_eq!(page_range(0, 247), 0..50);
        assert_eq!(page_range(1, 247), 50..100);
        assert_eq!(page_range(4, 247), 200..247);
    }

    #[test]
    fn a_page_past_the_end_shows_the_last_page() {
        assert_eq!(page_range(3, 9), 0..9);
        assert_eq!(page_range(7, 120), 100..120);
    }

    #[test]
    fn an_empty_list_is_one_empty_page() {
        assert_eq!(page_range(0, 0), 0..0);
    }

    #[test]
    fn the_pager_says_where_the_owner_is() {
        let html = render(|| {
            let page = Signal::new(1);
            rsx! { Pager { page, len: 247 } }
        });

        assert!(html.contains("Page 2 of 5"), "{html}");
        assert!(html.contains("Previous"), "{html}");
        assert!(html.contains("Next"), "{html}");
    }

    #[test]
    fn the_ends_disable_the_way_past_them() {
        let html = render(|| {
            let page = Signal::new(0);
            rsx! { Pager { page, len: 60 } }
        });

        let previous = html.split("Previous").next().unwrap();
        assert!(previous.contains("disabled"), "{html}");
        let next = html.split("Page 1 of 2").nth(1).unwrap();
        assert!(!next.contains("disabled"), "{html}");
    }

    #[test]
    fn a_list_that_fits_on_one_page_has_no_pager() {
        let html = render(|| {
            let page = Signal::new(0);
            rsx! { Pager { page, len: 50 } }
        });

        assert!(!html.contains("Page"), "{html}");
    }
}
