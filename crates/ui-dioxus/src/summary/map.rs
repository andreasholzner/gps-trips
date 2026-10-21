//! The tag summary's map (US-78): everything listed under the chosen tags on
//! one map — trips, and collections with their tracks and waypoints
//! (US-107) — as a share's overview map draws them (US-53).

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, TagSummaries};

use super::{entries, lines, Viewer};
use crate::activity_color::ActivityLegend;
use crate::api::{self, ApiClient};
use crate::interop::{self, MapWaypoint, OverviewEvent};
use crate::map_items::MapSource;
use crate::Route;

/// The map; clicking a line opens that trip as `viewer` sees it, or that
/// collection track's page, and choosing a waypoint opens its page. With one
/// tag, which color is which activity sits under it (US-75); with several,
/// each tag's color sits beside its name instead.
#[component]
pub fn SummaryMap(summary: TagSummaries, viewer: Viewer) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut shown = use_signal(Vec::<ActivityType>::new);
    let one_tag = summary.tags.len() == 1;
    let _draw = use_resource(use_reactive!(|summary, viewer| async move {
        let drawn = entries::drawn(&summary);
        let ids = |source| -> Vec<i64> {
            drawn
                .iter()
                .filter(|entry| entry.source == source)
                .map(|entry| entry.id)
                .collect()
        };
        let collections: Vec<i64> = summary.collections.iter().map(|c| c.id).collect();
        // One that cannot be read is absent from the answer and leaves its
        // line off the map; a request that fails outright leaves the map
        // without those lines, and the figures still stand.
        let trips = api::list_tracks(&archive(), &ids(MapSource::Trips))
            .await
            .unwrap_or_else(|err| {
                dioxus::logger::tracing::error!("could not read the tracks: {err}");
                Vec::new()
            });
        let tracks =
            api::list_collection_track_positions(&archive(), &ids(MapSource::CollectionTracks))
                .await
                .unwrap_or_else(|err| {
                    dioxus::logger::tracing::error!("could not read the collection tracks: {err}");
                    Vec::new()
                });
        let waypoints = api::list_waypoints_of_collections(&archive(), &collections)
            .await
            .unwrap_or_else(|err| {
                dioxus::logger::tracing::error!("could not read the waypoints: {err}");
                Vec::new()
            });
        let lines = lines::lines(&drawn, &trips, &tracks);
        shown.set(
            drawn
                .iter()
                .filter(|entry| {
                    lines
                        .iter()
                        .any(|line| line.source == Some(entry.source) && line.id == entry.id)
                })
                .map(|entry| entry.activity)
                .collect(),
        );
        let waypoints = waypoints
            .iter()
            .flat_map(|collection| &collection.waypoints)
            .map(MapWaypoint::from)
            .collect();
        let lines = interop::opening_pages(lines, |event| page_of(&viewer, event));
        let mut map = interop::start_overview_map(lines, waypoints, Default::default());
        while let Ok(event) = map.recv::<OverviewEvent>().await {
            if let Some(to) = page_of(&viewer, event) {
                navigator().push(to);
            }
        }
    }));
    rsx! {
        div { id: "overview-map", class: "overview-map" }
        if one_tag {
            ActivityLegend { shown: shown() }
        }
    }
}

/// The page a line or a waypoint chosen on the summary's map opens, for
/// `viewer`. Every line is given its source, and no links are drawn.
pub(super) fn page_of(viewer: &Viewer, event: OverviewEvent) -> Option<Route> {
    match event {
        OverviewEvent::OpenItem(opened) => Some(match opened.source {
            MapSource::Trips => viewer.trip_route(opened.id),
            MapSource::CollectionTracks => Route::CollectionTrack { id: opened.id },
        }),
        OverviewEvent::OpenWaypoint(id) => Some(Route::Waypoint { id }),
        _ => None,
    }
}
