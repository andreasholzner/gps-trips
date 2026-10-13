//! Selecting rows of a table to act on them together: the trips of the trip
//! list (US-34), and collections or a collection's tracks (US-94). The
//! selection is the caller's, so it outlives the page of rows shown.

use std::collections::BTreeSet;

use dioxus::prelude::*;

/// The header's checkbox: checked when every one of `listed` is selected,
/// and selecting or clearing them all. It acts on the rows actually listed,
/// so a filtered list can't select one the owner cannot see.
#[component]
pub fn SelectAll(listed: Vec<i64>, selected: Signal<BTreeSet<i64>>) -> Element {
    let all = !listed.is_empty() && listed.iter().all(|id| selected.read().contains(id));
    rsx! {
        input {
            r#type: "checkbox",
            "aria-label": "Select all",
            checked: all,
            onchange: move |event: FormEvent| {
                let mut selected = selected.write();
                for id in &listed {
                    if event.checked() {
                        selected.insert(*id);
                    } else {
                        selected.remove(id);
                    }
                }
            },
        }
    }
}

/// One row's checkbox.
#[component]
pub fn SelectOne(id: i64, selected: Signal<BTreeSet<i64>>) -> Element {
    rsx! {
        input {
            r#type: "checkbox",
            "aria-label": "Select",
            checked: selected.read().contains(&id),
            onchange: move |event: FormEvent| {
                if event.checked() {
                    selected.write().insert(id);
                } else {
                    selected.write().remove(&id);
                }
            },
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[test]
    fn select_all_is_checked_only_when_every_listed_row_is() {
        let html = render(|| {
            let selected = use_signal(|| BTreeSet::from([1, 2]));
            rsx! { SelectAll { listed: vec![1, 2], selected } }
        });
        assert!(html.contains("checked"), "{html}");

        let html = render(|| {
            let selected = use_signal(|| BTreeSet::from([1]));
            rsx! { SelectAll { listed: vec![1, 2], selected } }
        });
        assert!(!html.contains("checked"), "{html}");

        let html = render(|| {
            let selected = use_signal(BTreeSet::new);
            rsx! { SelectAll { listed: Vec::new(), selected } }
        });
        assert!(!html.contains("checked"), "nothing listed: {html}");
    }

    #[test]
    fn a_row_is_checked_when_selected() {
        let html = render(|| {
            let selected = use_signal(|| BTreeSet::from([4]));
            rsx! {
                SelectOne { id: 4, selected }
                SelectOne { id: 5, selected }
            }
        });
        assert_eq!(html.matches("checked").count(), 1, "{html}");
    }
}
