//! The bar acting on what is selected on the trip list (US-90): how many of
//! what, then the actions — tagging (US-34/100), sharing (US-53/94),
//! setting the activity (US-63) and linking (US-99) — each opening its
//! panel. An
//! action is inactive while the selection holds what it does not act on
//! ([`crate::picked`]).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::Tag;

use crate::api::BulkTagged;
use crate::bulk_activity::BulkActivityPanel;
use crate::bulk_tag::BulkTagPanel;
use crate::link_panel::LinkPanel;
use crate::picked::{self, Action, Picked, Target};
use crate::share::{Selected, ShareDialog};

/// Nothing until something is selected. `on_applied` says what an applied
/// tag or activity changed, for the list to read it anew; the selection is
/// cleared here first — except after a tag, which leaves it for the next
/// one (US-112).
#[component]
pub fn SelectionBar(
    picked: Signal<BTreeSet<Picked>>,
    all_tags: Vec<Tag>,
    on_applied: EventHandler<Selected>,
) -> Element {
    let mut open = use_signal(|| None::<Action>);
    // A selection that changed under an open panel closes it — emptied, or
    // now holding what its action does not act on — so it does not reopen
    // by itself once the selection allows it again.
    use_effect(move || {
        let still = stays_open(*open.peek(), &picked.read());
        if still != *open.peek() {
            open.set(still);
        }
    });
    let selection = picked.read().clone();
    if selection.is_empty() {
        return rsx! {};
    }
    let shown = stays_open(open(), &selection);
    let target = picked::target(&selection);
    let mut applied = move |of: &[Selected]| {
        picked.write().clear();
        open.set(None);
        for &of in of {
            on_applied.call(of);
        }
    };
    // Linking marks planned trips and collection tracks anew (US-99).
    let linked: &[Selected] = if selection.iter().any(|p| matches!(p, Picked::Track(_))) {
        &[Selected::Trips, Selected::CollectionTracks]
    } else {
        &[Selected::Trips]
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
        if shown == Some(Action::Link) {
            div { class: "bulk-panels",
                LinkPanel { picked: selection.clone(), on_linked: move |_| applied(linked) }
            }
        }
        if let (Some(action), Some(Target { of, ids })) = (shown, target) {
            match action {
                Action::Tag => rsx! {
                    div { class: "bulk-panels",
                        BulkTagPanel {
                            ids,
                            all_tags,
                            of: if of == Selected::Collections { BulkTagged::Collections } else { BulkTagged::Trips },
                            on_added: move |_| on_applied.call(of),
                        }
                    }
                },
                Action::SetActivity => rsx! {
                    div { class: "bulk-panels",
                        BulkActivityPanel { ids, on_applied: move |_| applied(&[of]) }
                    }
                },
                Action::Share => rsx! {
                    ShareDialog { target: of.target(ids), on_close: move |_| open.set(None) }
                },
                // Over pairs rather than one target: above.
                Action::Link => rsx! {},
            }
        }
    }
}

/// The panel open for `picked`: `open`, while its action can be taken.
fn stays_open(open: Option<Action>, picked: &BTreeSet<Picked>) -> Option<Action> {
    open.filter(|action| action.active(picked))
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
            rsx! { SelectionBar { picked, all_tags: Vec::new(), on_applied: |_| {} } }
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
        for label in ["Tag…", "Share…", "Set activity…", "Link…"] {
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
        for label in ["Tag…", "Share…", "Set activity…", "Link…"] {
            assert!(button(&html, label).contains("disabled"), "{html}");
        }
    }

    #[test]
    fn us99_a_recorded_trip_and_a_collection_track_can_be_linked_and_nothing_else_done() {
        let html = bar(&[Picked::Trip(Kind::Recorded, 1), Picked::Track(4)]);

        assert!(!button(&html, "Link…").contains("disabled"), "{html}");
        for label in ["Tag…", "Share…", "Set activity…"] {
            assert!(button(&html, label).contains("disabled"), "{html}");
        }
        let two_recorded = bar(&[
            Picked::Trip(Kind::Recorded, 1),
            Picked::Trip(Kind::Recorded, 2),
        ]);
        let link = button(&two_recorded, "Link…");
        assert!(link.contains("disabled"), "{two_recorded}");
        assert!(
            link.contains("Select recorded trips with planned trips"),
            "{two_recorded}"
        );
    }

    #[test]
    fn us90_a_panel_stays_open_only_while_its_action_can_be_taken() {
        let trips: BTreeSet<Picked> = [Picked::Trip(Kind::Recorded, 1)].into();
        let mixed: BTreeSet<Picked> =
            [Picked::Trip(Kind::Recorded, 1), Picked::Collection(3)].into();

        assert_eq!(stays_open(Some(Action::Tag), &trips), Some(Action::Tag));
        assert_eq!(stays_open(None, &trips), None);
        // Closed, not merely hidden: it does not reopen by itself once the
        // selection allows it again.
        assert_eq!(stays_open(Some(Action::Tag), &mixed), None);
        assert_eq!(stays_open(Some(Action::Tag), &BTreeSet::new()), None);
    }
}
