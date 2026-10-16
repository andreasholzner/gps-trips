//! What a page's links hold of proposals (US-103): the item a page opens
//! with ticked, and the proposals ticked on the map, confirmed or dismissed
//! together below the list.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::LinkOther;

use super::{Group, Shown};
use crate::api::LinkEnd;
use crate::kinds::Kind;

/// The row of `other` among `groups`, as the map shows it.
pub(super) fn to_tick(other: LinkOther, groups: &[Group]) -> Option<Shown> {
    groups
        .iter()
        .flat_map(|group| &group.rows)
        .map(|row| row.shown)
        .find(|shown| match (other, *shown) {
            (LinkOther::Trip(id), Shown::Line(kind, line)) => {
                kind != Kind::Collections && id == line
            }
            (LinkOther::CollectionTrack(id), Shown::Line(kind, line)) => {
                kind == Kind::Collections && id == line
            }
            (LinkOther::Waypoint(id), Shown::Waypoint(waypoint)) => id == waypoint,
            _ => false,
        })
}

/// The proposals among `groups` ticked to be shown on the map — what
/// confirming or dismissing the ticked acts on.
pub(super) fn ticked_proposals(groups: &[Group], shown: &BTreeSet<Shown>) -> Vec<(i64, LinkEnd)> {
    groups
        .iter()
        .flat_map(|group| &group.rows)
        .filter(|row| row.proposal.is_some() && shown.contains(&row.shown))
        .filter_map(|row| row.unlink)
        .collect()
}

/// Below the list, when it holds a proposal the owner may settle: confirm or
/// dismiss every proposal ticked on the map at once.
#[component]
pub(super) fn SettleTicked(
    groups: Vec<Group>,
    shown: Signal<BTreeSet<Shown>>,
    on_settle: EventHandler<(bool, Vec<(i64, LinkEnd)>)>,
) -> Element {
    let settleable = groups
        .iter()
        .flat_map(|group| &group.rows)
        .any(|row| row.proposal.is_some() && row.unlink.is_some());
    if !settleable {
        return rsx! {};
    }
    let ticked = ticked_proposals(&groups, &shown.read());
    let none = ticked.is_empty();
    let dismissed = ticked.clone();
    rsx! {
        div { class: "settle-ticked",
            button {
                id: "confirm-ticked",
                r#type: "button",
                class: "quiet",
                disabled: none,
                onclick: move |_| on_settle.call((true, ticked.clone())),
                "Confirm ticked"
            }
            button {
                id: "dismiss-ticked",
                r#type: "button",
                class: "quiet",
                disabled: none,
                onclick: move |_| on_settle.call((false, dismissed.clone())),
                "Dismiss ticked"
            }
        }
    }
}
