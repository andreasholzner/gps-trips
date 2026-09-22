//! Setting one activity type on the trips selected on the list screen
//! (US-63), beside bulk tagging (US-34).
//!
//! It differs from tagging in what the screen has to respect: a tag is
//! added, an activity type *overwrites* one. So the panel says how many
//! trips it will change and asks before it does.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::ActivityType;

use crate::api::{self, ApiClient};

/// What the panel asks before overwriting. `Unknown` is named the way the
/// owner would say it rather than by its picker label, which is dashes.
pub fn confirmation(count: usize, activity: ActivityType) -> String {
    let trips = if count == 1 { "trip" } else { "trips" };
    let to = match activity {
        ActivityType::Unknown => "unspecified",
        other => other.label(),
    };
    format!(
        "Set the activity of {count} {trips} to {to}? This replaces the activity they have now."
    )
}

/// The bulk activity panel (US-63). Appears only once trips are selected;
/// offers the same closed set the edit form does (ADR-0018), and applies
/// the chosen one to every selected trip in one request once confirmed.
#[component]
pub fn BulkActivityPanel(selected: Signal<BTreeSet<i64>>, on_applied: EventHandler<()>) -> Element {
    let mut chosen = use_signal(|| None::<ActivityType>);
    let mut confirming = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);
    let archive = use_context::<Signal<ApiClient>>();

    let count = selected.read().len();
    if count == 0 {
        return rsx! {};
    }

    rsx! {
        fieldset {
            legend { "Set activity of selected trips" }
            div { class: "tag-entry",
                select {
                    "aria-label": "Activity for selected trips",
                    value: chosen().map_or("", |activity| activity.as_str()),
                    onchange: move |event| {
                        chosen.set(event.value().parse::<ActivityType>().ok());
                        confirming.set(false);
                    },
                    // Nothing is chosen until the owner picks: "unspecified"
                    // is a real choice here — it clears the trips' activity —
                    // not a default to apply by accident.
                    option { value: "", "Choose…" }
                    option { value: ActivityType::Unknown.as_str(), "{ActivityType::Unknown.label()}" }
                    for activity in ActivityType::SELECTABLE {
                        option { key: "{activity}", value: activity.as_str(), "{activity.label()}" }
                    }
                }
                button {
                    r#type: "button",
                    disabled: chosen().is_none(),
                    onclick: move |_| confirming.set(true),
                    "Set for {count} selected"
                }
            }

            if let (true, Some(activity)) = (confirming(), chosen()) {
                p {
                    "{confirmation(count, activity)}"
                    button {
                        r#type: "button",
                        onclick: move |_| {
                            let trip_ids: Vec<i64> = selected.read().iter().copied().collect();
                            spawn(async move {
                                match api::bulk_set_activity_type(&archive(), &trip_ids, activity).await {
                                    Ok(()) => {
                                        selected.write().clear();
                                        chosen.set(None);
                                        confirming.set(false);
                                        message.set(None);
                                        on_applied.call(());
                                    }
                                    Err(err) => {
                                        confirming.set(false);
                                        message.set(Some(err.to_string()));
                                    }
                                }
                            });
                        },
                        "Change"
                    }
                    button {
                        r#type: "button",
                        onclick: move |_| confirming.set(false),
                        "Cancel"
                    }
                }
            }

            if let Some(message) = message() {
                p { class: "error", "{message}" }
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
    fn the_confirmation_says_how_many_trips_it_will_change_and_to_what() {
        assert_eq!(
            confirmation(3, ActivityType::SkiTouring),
            "Set the activity of 3 trips to Ski touring? This replaces the activity they have now."
        );
    }

    #[test]
    fn one_trip_is_one_trip() {
        assert!(confirmation(1, ActivityType::Hiking).contains("of 1 trip to"));
    }

    #[test]
    fn clearing_the_activity_is_said_in_words() {
        assert!(
            confirmation(2, ActivityType::Unknown).contains("to unspecified?"),
            "{}",
            confirmation(2, ActivityType::Unknown)
        );
    }

    #[test]
    fn the_panel_offers_the_edit_forms_activities_for_the_selected_trips() {
        let html = render(|| {
            let selected = Signal::new(BTreeSet::from([1_i64, 2]));
            rsx! { BulkActivityPanel { selected, on_applied: |_| {} } }
        });

        assert!(html.contains("Set for 2 selected"), "{html}");
        assert!(html.contains(ActivityType::Unknown.label()), "{html}");
        for activity in ActivityType::SELECTABLE {
            assert!(html.contains(activity.label()), "{activity}: {html}");
        }
        // Nothing is applied before something is chosen.
        assert!(html.contains("disabled"), "{html}");
    }

    #[test]
    fn the_panel_stays_out_of_the_way_until_trips_are_selected() {
        let html = render(|| {
            let selected = Signal::new(BTreeSet::new());
            rsx! { BulkActivityPanel { selected, on_applied: |_| {} } }
        });

        assert!(!html.contains("Set for"), "{html}");
    }
}
