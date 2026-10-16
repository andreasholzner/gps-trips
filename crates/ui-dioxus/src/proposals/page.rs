//! The Proposed links page (US-103): every link the archive proposed, each
//! leading to its trip's page with the other end already ticked on the map,
//! and a selection confirmed or dismissed in one action.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, LinkPair, ProposedEnd, ProposedLink};

use super::{coverage, use_proposals_changed, ProposalsChanged};
use crate::activity_icon::ActivityIcon;
use crate::api::{self, ApiClient};
use crate::detail::Showing;
use crate::linked::trip_date;
use crate::waypoint::UNNAMED;
use crate::waypoint_icon::WaypointSymbol;
use crate::Route;

/// Whether every one of `listed` is selected — and there is one to select.
pub fn all_selected(listed: &[ProposedLink], selected: &BTreeSet<LinkPair>) -> bool {
    !listed.is_empty() && listed.iter().all(|link| selected.contains(&link.pair()))
}

/// The screen.
#[component]
pub fn ProposedLinks() -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let changed = use_proposals_changed();
    // Read again whenever proposals change elsewhere — a check started, or
    // one confirmed on a trip's page.
    let mut listed = use_resource(move || {
        if let Some(ProposalsChanged(changed)) = changed {
            changed.read();
        }
        async move { api::list_proposals(&archive()).await }
    });
    let mut selected = use_signal(BTreeSet::<LinkPair>::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let settle = move |confirm: bool| async move {
        let pairs: Vec<LinkPair> = selected.read().iter().copied().collect();
        if pairs.is_empty() || busy() {
            return;
        }
        busy.set(true);
        let result = if confirm {
            api::confirm_proposals(&archive(), &pairs).await
        } else {
            api::dismiss_proposals(&archive(), &pairs).await
        };
        busy.set(false);
        match result {
            Ok(()) => error.set(None),
            // All or nothing: one gone meanwhile leaves the rest proposed,
            // and the list read again shows what is.
            Err(err) => error.set(Some(err.to_string())),
        }
        selected.write().clear();
        listed.restart();
        if let Some(changed) = changed {
            changed.bump();
        }
    };

    rsx! {
        h1 { "Proposed links" }
        match &*listed.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the proposals: {err}" } },
            Some(Ok(links)) if links.is_empty() => rsx! {
                p { "Nothing is proposed. Proposals come when a trip or a collection enters the archive, or when you look for links from its page." }
            },
            Some(Ok(links)) => {
                let links = links.clone();
                let all = all_selected(&links, &selected.read());
                let count = selected.read().len();
                let pairs: Vec<LinkPair> = links.iter().map(ProposedLink::pair).collect();
                rsx! {
                    div { class: "proposal-actions",
                        label {
                            input {
                                r#type: "checkbox",
                                "aria-label": "Select all",
                                checked: all,
                                onchange: move |event: FormEvent| {
                                    let mut selected = selected.write();
                                    for pair in &pairs {
                                        if event.checked() {
                                            selected.insert(*pair);
                                        } else {
                                            selected.remove(pair);
                                        }
                                    }
                                },
                            }
                            " {count} of {links.len()} selected"
                        }
                        button {
                            id: "confirm-proposals",
                            r#type: "button",
                            disabled: count == 0 || busy(),
                            onclick: move |_| settle(true),
                            "Confirm"
                        }
                        button {
                            id: "dismiss-proposals",
                            r#type: "button",
                            class: "secondary",
                            disabled: count == 0 || busy(),
                            onclick: move |_| settle(false),
                            "Dismiss"
                        }
                    }
                    if let Some(message) = error() {
                        p { class: "error", "Could not settle the selection: {message}" }
                    }
                    ul { class: "proposals",
                        for link in links {
                            Entry { key: "{link.pair().trip_id}-{link.pair().other:?}", link, selected }
                        }
                    }
                }
            }
        }
    }
}

/// One proposal: its checkbox, its trip leading to the trip's page with the
/// other end ticked, the other end, and what it was proposed on.
#[component]
fn Entry(link: ProposedLink, selected: Signal<BTreeSet<LinkPair>>) -> Element {
    let pair = link.pair();
    let trip = &link.trip;
    let trip_date = trip_date(link.trip_kind, &trip.name, trip.start_date.as_deref());
    let (other_name, other_detail, other_icon) = match &link.other {
        ProposedEnd::PlannedTrip(plan) => (plan.name.clone(), None, Some(plan.activity_type)),
        ProposedEnd::CollectionTrack(track) => (
            track.name.clone(),
            Some(track.collection_name.clone()),
            Some(track.activity_type),
        ),
        ProposedEnd::Waypoint(waypoint) => (
            waypoint.name.clone().unwrap_or_else(|| UNNAMED.to_string()),
            Some(waypoint.collection_name.clone()),
            None,
        ),
    };
    let symbol = match &link.other {
        ProposedEnd::Waypoint(waypoint) => Some(waypoint.symbol.clone()),
        _ => None,
    };
    let to = Route::TripShowing {
        id: trip.id,
        show: Showing(pair.other),
    };
    rsx! {
        li {
            input {
                r#type: "checkbox",
                "aria-label": "Select {trip.name} and {other_name}",
                checked: selected.read().contains(&pair),
                onchange: move |event: FormEvent| {
                    if event.checked() {
                        selected.write().insert(pair);
                    } else {
                        selected.write().remove(&pair);
                    }
                },
            }
            Link { to,
                Icon { activity: trip.activity_type }
                "{trip.name}"
            }
            if let Some(date) = trip_date {
                span { class: "muted", " · {date}" }
            }
            span { class: "muted", " → " }
            match (other_icon, symbol) {
                (Some(activity), _) => rsx! { Icon { activity } },
                (None, Some(symbol)) => rsx! { WaypointSymbol { symbol } },
                (None, None) => rsx! {},
            }
            "{other_name}"
            if let Some(detail) = other_detail {
                span { class: "muted", " · {detail}" }
            }
            if let Some(coverage) = coverage(&link.proposal) {
                span { class: "muted", " · {coverage}" }
            }
        }
    }
}

/// An activity's icon, none for an unknown one — as a page's links show it.
#[component]
fn Icon(activity: ActivityType) -> Element {
    match activity {
        ActivityType::Unknown => rsx! {},
        activity => rsx! { ActivityIcon { activity } },
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
