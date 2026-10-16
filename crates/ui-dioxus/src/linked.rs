//! What a trip, a collection track or a waypoint is linked to (US-99), on
//! its page: each other end with its activity's icon, leading to its own
//! page, a recorded trip with its date unless its name starts with it, a
//! box showing it on the page's map, and a control removing the link at
//! once — it can be linked again. A share's recipient sees the shared trips
//! a shared trip is linked to, without the control.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{
    ActivityType, Links, Proposal, RecordedTripFilter, SharedLinks, TripKind,
};

use crate::activity_icon::ActivityIcon;
use crate::api::{self, ApiClient, LinkEnd};
use crate::interop::Overlays;
use crate::kinds::Kind;
use crate::proposals::{coverage, use_proposals_changed, ProposalsChanged};
use crate::waypoint::UNNAMED;
use crate::waypoint_icon::WaypointSymbol;
use crate::Route;

mod overlays;
use overlays::overlays_of;

/// One kind of other end, as its heading names it.
#[derive(Clone, PartialEq)]
pub struct Group {
    pub title: &'static str,
    pub rows: Vec<Row>,
}

/// One other end.
#[derive(Clone, PartialEq)]
pub struct Row {
    pub name: String,
    /// The date a recorded trip started on, unless its name starts with it;
    /// the collection a track or a waypoint belongs to.
    pub detail: Option<String>,
    pub to: Route,
    pub icon: RowIcon,
    /// What its box puts on the page's map.
    pub shown: Shown,
    /// The trip whose link removing it names, and the link's other end.
    pub unlink: Option<(i64, LinkEnd)>,
    /// What it was proposed on, while it is a proposal (US-103); removing
    /// one dismisses it.
    pub proposal: Option<Proposal>,
}

/// What goes in front of a row's name.
#[derive(Clone, PartialEq)]
pub enum RowIcon {
    /// A trip's or a track's activity.
    Activity(ActivityType),
    /// A waypoint's symbol (US-95), drawn as on its collection's map.
    Symbol(Option<String>),
}

/// Something the page's map can show: a trip's line, of its kind, or a
/// collection track's (`Kind::Collections`); or a waypoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shown {
    Line(Kind, i64),
    Waypoint(i64),
}

/// The date shown beside a trip: a recorded one's, unless its name already
/// starts with it — as a name the import suggested does (US-12).
pub fn trip_date(kind: TripKind, name: &str, start_date: Option<&str>) -> Option<String> {
    match (kind, start_date) {
        (TripKind::Recorded, Some(date)) if !name.starts_with(date) => Some(date.to_string()),
        _ => None,
    }
}

/// One linked trip's row, leading to `to`.
fn trip_row(
    kind: TripKind,
    trip: (i64, &str, ActivityType, Option<&str>),
    to: Route,
    unlink: Option<(i64, LinkEnd)>,
    proposal: Option<Proposal>,
) -> Row {
    let (id, name, activity, start_date) = trip;
    Row {
        name: name.to_string(),
        detail: trip_date(kind, name, start_date),
        to,
        icon: RowIcon::Activity(activity),
        shown: Shown::Line(Kind::of_trip(kind), id),
        unlink,
        proposal,
    }
}

/// The groups `end`'s page lists, in the order it lists them, leaving out
/// the empty ones.
pub fn groups(end: LinkEnd, links: &Links) -> Vec<Group> {
    // From a trip's page, the trip names every link; from anything else's,
    // the trip listed does — and only trips are listed there.
    let own_trip = match end {
        LinkEnd::Trip(own) => Some(own),
        LinkEnd::CollectionTrack(_) | LinkEnd::Waypoint(_) => None,
    };
    let trips = |kind: TripKind, trips: &[trip_archive_types::LinkedTrip]| -> Vec<Row> {
        trips
            .iter()
            .map(|trip| {
                let unlink = match own_trip {
                    Some(own) => (own, LinkEnd::Trip(trip.id)),
                    None => (trip.id, end),
                };
                trip_row(
                    kind,
                    (
                        trip.id,
                        &trip.name,
                        trip.activity_type,
                        trip.start_date.as_deref(),
                    ),
                    Route::TripDetail { id: trip.id },
                    Some(unlink),
                    trip.proposal,
                )
            })
            .collect()
    };
    let tracks = links
        .collection_tracks
        .iter()
        .map(|track| Row {
            name: track.name.clone(),
            detail: Some(track.collection_name.clone()),
            to: Route::CollectionTrack { id: track.id },
            icon: RowIcon::Activity(track.activity_type),
            shown: Shown::Line(Kind::Collections, track.id),
            unlink: own_trip.map(|own| (own, LinkEnd::CollectionTrack(track.id))),
            proposal: track.proposal,
        })
        .collect();
    let waypoints = links
        .waypoints
        .iter()
        .map(|waypoint| Row {
            name: waypoint.name.clone().unwrap_or_else(|| UNNAMED.to_string()),
            detail: Some(waypoint.collection_name.clone()),
            to: Route::Waypoint { id: waypoint.id },
            icon: RowIcon::Symbol(waypoint.symbol.clone()),
            shown: Shown::Waypoint(waypoint.id),
            unlink: own_trip.map(|own| (own, LinkEnd::Waypoint(waypoint.id))),
            proposal: waypoint.proposal,
        })
        .collect();
    [
        Group {
            title: "Recorded trips",
            rows: trips(TripKind::Recorded, &links.recorded_trips),
        },
        Group {
            title: "Planned trips",
            rows: trips(TripKind::Planned, &links.planned_trips),
        },
        Group {
            title: "Collection tracks",
            rows: tracks,
        },
        Group {
            title: "Waypoints",
            rows: waypoints,
        },
    ]
    .into_iter()
    .filter(|group| !group.rows.is_empty())
    .collect()
}

/// The groups a share's recipient reads of a shared trip, leading to the
/// share's own pages.
pub fn shared_groups(token: &str, links: &SharedLinks) -> Vec<Group> {
    let trips = |kind: TripKind, trips: &[trip_archive_types::SharedLinkedTrip]| -> Vec<Row> {
        trips
            .iter()
            .map(|trip| {
                trip_row(
                    kind,
                    (
                        trip.id,
                        &trip.name,
                        trip.activity_type,
                        trip.start_date.as_deref(),
                    ),
                    Route::SharedTripDetail {
                        token: token.to_string(),
                        id: trip.id,
                    },
                    None,
                    None,
                )
            })
            .collect()
    };
    [
        Group {
            title: "Recorded trips",
            rows: trips(TripKind::Recorded, &links.recorded_trips),
        },
        Group {
            title: "Planned trips",
            rows: trips(TripKind::Planned, &links.planned_trips),
        },
    ]
    .into_iter()
    .filter(|group| !group.rows.is_empty())
    .collect()
}

/// What marks a planned trip, a collection track or a waypoint a recorded
/// trip is linked to — however little of it the trip covered — in a list.
pub const RECORDED_TRIP_MARK: &str = "Has a recorded trip";

/// The mark itself: a glyph, named on hover and to a screen reader.
#[component]
pub fn RecordedTripMark() -> Element {
    rsx! {
        span { class: "recorded-mark", title: RECORDED_TRIP_MARK,
            span { aria_hidden: "true", " ✓" }
            span { class: "visually-hidden", " ({RECORDED_TRIP_MARK})" }
        }
    }
}

/// The choice between everything, what has a recorded trip and what has
/// none — the trip list's filter and a collection page's.
#[component]
pub fn RecordedTripChoice(
    id: &'static str,
    value: Option<RecordedTripFilter>,
    onchange: EventHandler<Option<RecordedTripFilter>>,
) -> Element {
    rsx! {
        label {
            "Recorded trip "
            select {
                id,
                onchange: move |event| onchange.call(event.value().parse().ok()),
                option { value: "", selected: value.is_none(), "— any —" }
                for (filter, label) in [
                    (RecordedTripFilter::With, "With one"),
                    (RecordedTripFilter::Without, "Without one"),
                ] {
                    option {
                        key: "{filter.as_str()}",
                        value: filter.as_str(),
                        selected: value == Some(filter),
                        "{label}"
                    }
                }
            }
        }
    }
}

/// The message of a failed removal, if it failed on `end`'s page.
pub fn error_on(end: LinkEnd, error: &Option<(LinkEnd, String)>) -> Option<&str> {
    match error {
        Some((failed_on, message)) if *failed_on == end => Some(message),
        _ => None,
    }
}

/// The owner's section: what `end` is linked to. Nothing while it is read,
/// nothing for no links, and nothing but a log line if it cannot be read —
/// the page is about something else. What the owner ticks is put into
/// `overlays`, for the page's map to draw.
#[component]
pub fn LinkedTo(end: LinkEnd, overlays: Signal<Overlays>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let changed = use_proposals_changed();
    // The end travels back with the answer: the router shows the next page
    // of the same kind through this same scope. Read again when proposals
    // change — a check started from the page's actions (US-103).
    let mut links = use_resource(use_reactive!(|end| {
        if let Some(ProposalsChanged(changed)) = changed {
            changed.read();
        }
        async move { (end, api::list_links(&archive(), end).await) }
    }));
    // A failed removal, with the page it failed on, for the same reason.
    let mut error = use_signal(|| None::<(LinkEnd, String)>);
    let shown = use_shown(end, overlays, move || match &*links.read() {
        Some((read_for, Ok(links))) => groups(*read_for, links),
        _ => Vec::new(),
    });
    let read = links.read_unchecked();
    let groups = match &*read {
        Some((read_for, Ok(links))) if *read_for == end => groups(end, links),
        Some((read_for, Err(err))) if *read_for == end => {
            dioxus::logger::tracing::error!("could not list the links: {err}");
            Vec::new()
        }
        _ => Vec::new(),
    };
    // Removing a proposal dismisses it, so either may change the count.
    let on_unlink = move |(trip, other): (i64, LinkEnd)| async move {
        match api::unlink(&archive(), trip, other).await {
            Ok(()) => {
                error.set(None);
                links.restart();
                changed.map(|changed| changed.bump());
            }
            Err(err) => error.set(Some((end, format!("Could not remove the link: {err}")))),
        }
    };
    let on_confirm = move |(trip, other): (i64, LinkEnd)| async move {
        match api::confirm_proposals(&archive(), &[other.pair_with(trip)]).await {
            Ok(()) => {
                error.set(None);
                links.restart();
                changed.map(|changed| changed.bump());
            }
            Err(err) => error.set(Some((end, format!("Could not confirm the link: {err}")))),
        }
    };
    rsx! {
        LinkGroups { groups, shown, on_unlink, on_confirm }
        if let Some(message) = error_on(end, &error.read()) {
            p { class: "error", "{message}" }
        }
    }
}

/// A share recipient's section: the shared trips a shared trip is linked
/// to, read on the share's own client, and shown on its map when ticked.
#[component]
pub fn SharedLinkedTo(token: String, id: i64, overlays: Signal<Overlays>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let links = use_resource(use_reactive!(|id| async move {
        (id, api::list_shared_trip_links(&archive(), id).await)
    }));
    let for_map = token.clone();
    let shown = use_shown(LinkEnd::Trip(id), overlays, move || match &*links.read() {
        Some((_, Ok(links))) => shared_groups(&for_map, links),
        _ => Vec::new(),
    });
    let groups = match &*links.read_unchecked() {
        Some((read_for, Ok(links))) if *read_for == id => shared_groups(&token, links),
        _ => Vec::new(),
    };
    rsx! {
        LinkGroups { groups, shown, on_unlink: |_| {} }
    }
}

/// What is ticked to be shown, emptied on a page of its own, and the
/// overlays it comes to, read from the archive whenever either the ticks or
/// the `listed` rows change — so an item whose link is removed leaves the
/// map too.
fn use_shown(
    end: LinkEnd,
    mut overlays: Signal<Overlays>,
    listed: impl Fn() -> Vec<Group> + 'static,
) -> Signal<BTreeSet<Shown>> {
    let archive = use_context::<Signal<ApiClient>>();
    let mut shown = use_signal(BTreeSet::<Shown>::new);
    use_effect(use_reactive!(|end| {
        let _ = end;
        shown.write().clear();
    }));
    let _read = use_resource(move || {
        let ticked: Vec<Row> = listed()
            .into_iter()
            .flat_map(|group| group.rows)
            .filter(|row| shown.read().contains(&row.shown))
            .collect();
        async move { overlays.set(overlays_of(&archive(), ticked).await) }
    });
    shown
}

#[component]
fn LinkGroups(
    groups: Vec<Group>,
    shown: Signal<BTreeSet<Shown>>,
    on_unlink: EventHandler<(i64, LinkEnd)>,
    #[props(default)] on_confirm: EventHandler<(i64, LinkEnd)>,
) -> Element {
    if groups.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { id: "links",
            h2 { "Linked" }
            for group in groups {
                h3 { key: "{group.title}", "{group.title}" }
                ul { class: "links",
                    for row in group.rows {
                        LinkRow { row, shown, on_unlink, on_confirm }
                    }
                }
            }
        }
    }
}

#[component]
fn LinkRow(
    row: Row,
    mut shown: Signal<BTreeSet<Shown>>,
    on_unlink: EventHandler<(i64, LinkEnd)>,
    on_confirm: EventHandler<(i64, LinkEnd)>,
) -> Element {
    let item = row.shown;
    rsx! {
        li {
            input {
                r#type: "checkbox",
                aria_label: "Show {row.name} on the map",
                title: "Show on the map",
                checked: shown.read().contains(&item),
                onchange: move |event| {
                    if event.checked() {
                        shown.write().insert(item);
                    } else {
                        shown.write().remove(&item);
                    }
                },
            }
            Link { to: row.to.clone(),
                match &row.icon {
                    RowIcon::Activity(ActivityType::Unknown) => rsx! {},
                    RowIcon::Activity(activity) => rsx! { ActivityIcon { activity: *activity } },
                    RowIcon::Symbol(symbol) => rsx! { WaypointSymbol { symbol: symbol.clone() } },
                }
                "{row.name}"
            }
            if let Some(detail) = &row.detail {
                span { class: "muted", " · {detail}" }
            }
            if let Some(proposal) = &row.proposal {
                " "
                span { class: "proposed", "proposed" }
                if let Some(coverage) = coverage(proposal) {
                    span { class: "muted", " · {coverage}" }
                }
            }
            if let (Some(pair), Some(_)) = (row.unlink, row.proposal) {
                " "
                button {
                    r#type: "button",
                    class: "confirm",
                    title: "Confirm the link to {row.name}",
                    aria_label: "Confirm the link to {row.name}",
                    onclick: move |_| on_confirm.call(pair),
                    "✓"
                }
            }
            if let Some(unlink) = row.unlink {
                " "
                button {
                    r#type: "button",
                    class: "unlink",
                    title: if row.proposal.is_some() { "Dismiss the proposed link to {row.name}" } else { "Remove the link to {row.name}" },
                    aria_label: if row.proposal.is_some() { "Dismiss the proposed link to {row.name}" } else { "Remove the link to {row.name}" },
                    onclick: move |_| on_unlink.call(unlink),
                    "×"
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;

#[cfg(test)]
mod proposal_tests;
