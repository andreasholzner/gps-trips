//! The bar acting on what is selected on the trip list (US-90): how many of
//! what, then the actions — tagging (US-34/100), sharing (US-53/94) and
//! setting the activity (US-63) — each opening the panel it always had. An
//! action is inactive while the selection holds what it does not act on
//! ([`crate::picked`]).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::Tag;

use crate::api::BulkTagged;
use crate::bulk_activity::BulkActivityPanel;
use crate::bulk_tag::BulkTagPanel;
use crate::picked::{self, Action, Picked, Target};
use crate::share::{Selected, ShareDialog};

/// Nothing until something is selected. `on_applied` says what an applied
/// tag or activity changed, for the list to read it anew; the selection is
/// cleared here first.
#[component]
pub fn SelectionBar(
    picked: Signal<BTreeSet<Picked>>,
    staged: Signal<Vec<String>>,
    all_tags: Vec<Tag>,
    on_applied: EventHandler<Selected>,
) -> Element {
    let mut open = use_signal(|| None::<Action>);
    let selection = picked.read().clone();
    if selection.is_empty() {
        return rsx! {};
    }
    // A selection that changed under an open panel can close it.
    let shown = open().filter(|action| action.active(&selection));
    let target = picked::target(&selection);
    let mut applied = move |of: Selected| {
        picked.write().clear();
        open.set(None);
        on_applied.call(of);
    };
    rsx! {
        div { class: "selection-bar",
            strong { {picked::summary(&selection)} }
            div { class: "selection-actions",
                for action in Action::ALL {
                    button {
                        key: "{action.label()}",
                        r#type: "button",
                        disabled: !action.active(&selection),
                        title: if !action.active(&selection) { action.needs() },
                        aria_pressed: "{shown == Some(action)}",
                        onclick: move |_| {
                            let next = (open() != Some(action)).then_some(action);
                            open.set(next);
                        },
                        {action.label()}
                    }
                }
            }
        }
        if let (Some(action), Some(Target { of, ids })) = (shown, target) {
            match action {
                Action::Tag => rsx! {
                    div { class: "bulk-panels",
                        BulkTagPanel {
                            ids,
                            staged,
                            all_tags,
                            of: if of == Selected::Collections { BulkTagged::Collections } else { BulkTagged::Trips },
                            on_applied: move |_| applied(of),
                        }
                    }
                },
                Action::SetActivity => rsx! {
                    div { class: "bulk-panels",
                        BulkActivityPanel { ids, on_applied: move |_| applied(of) }
                    }
                },
                Action::Share => rsx! {
                    ShareDialog { target: of.target(ids), on_close: move |_| open.set(None) }
                },
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::Kind;
    use crate::test_support::render;

    fn bar(picked: &[Picked]) -> String {
        let picked: BTreeSet<Picked> = picked.iter().copied().collect();
        render(move || {
            let picked = use_signal(|| picked.clone());
            let staged = use_signal(Vec::new);
            rsx! { SelectionBar { picked, staged, all_tags: Vec::new(), on_applied: |_| {} } }
        })
    }

    fn button<'a>(html: &'a str, label: &str) -> &'a str {
        html.split("<button")
            .find(|button| button.contains(label))
            .unwrap_or_else(|| panic!("no {label} button: {html}"))
    }

    #[test]
    fn us90_the_bar_stays_out_of_the_way_until_something_is_selected() {
        let html = bar(&[]);

        assert!(!html.contains("selection-bar"), "{html}");
    }

    #[test]
    fn us90_the_bar_says_what_is_selected_and_offers_every_action() {
        let html = bar(&[
            Picked::Trip(Kind::Recorded, 1),
            Picked::Trip(Kind::Planned, 2),
        ]);

        assert!(html.contains("2 selected: 2 trips"), "{html}");
        for label in ["Tag…", "Share…", "Set activity…"] {
            assert!(!button(&html, label).contains("disabled"), "{html}");
        }
        // Each opens its panel, closed to begin with.
        assert!(!html.contains("Tag selected"), "{html}");
        assert!(!html.contains("Set activity of selected"), "{html}");
    }

    #[test]
    fn us90_an_action_is_inactive_on_what_it_does_not_act_on() {
        let html = bar(&[Picked::Track(4)]);

        assert!(!button(&html, "Share…").contains("disabled"), "{html}");
        let tag = button(&html, "Tag…");
        assert!(tag.contains("disabled"), "{html}");
        assert!(
            tag.contains("Select only trips, or only collections"),
            "{html}"
        );
        assert!(
            button(&html, "Set activity…").contains("disabled"),
            "{html}"
        );
    }

    #[test]
    fn us90_every_action_is_inactive_on_a_mixed_selection() {
        let html = bar(&[Picked::Trip(Kind::Recorded, 1), Picked::Collection(3)]);

        assert!(html.contains("2 selected: 1 trip · 1 collection"), "{html}");
        for label in ["Tag…", "Share…", "Set activity…"] {
            assert!(button(&html, label).contains("disabled"), "{html}");
        }
    }
}
