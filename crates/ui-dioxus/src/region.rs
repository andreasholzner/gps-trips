//! The region map (US-52, carrying US-14; made the screen's centre by
//! US-63): the trips the filters match, drawn as a heat map — or, zoomed
//! in, as their tracks (US-73). With "Filter to map" ticked, its visible
//! area is the region (US-92): the list narrows to trips whose stored
//! bounding box overlaps it, and follows every pan and zoom. The listed
//! collections' tracks (US-87) are drawn with the trips, each kind in its
//! line style (US-90), and the region narrows the collections too.
//!
//! The map itself is Leaflet, reached through `interop`; this module is the
//! Rust half — what to hand the map, and what to do with the view it
//! reports back.

use std::collections::HashSet;

use dioxus::prelude::*;
use trip_archive_types::ActivityType;

use crate::activity_color::ActivityLegend;
use crate::api::{self, ApiClient};
use crate::filters::Filters;
use crate::heat;
use crate::interop::{self, RegionEvent};
use crate::map_items::{MapItem, MapSource, Mapped};
use crate::trip_lines::{self, TrackCache, Viewport, WaypointCache};
use crate::Route;

/// The map and its controls, always in view (US-63). `mapped` — every trip
/// the filters match, or every listed collection's track (US-87) — is
/// `None` until the list has been read, so the map is not wiped blank while
/// a re-query is in flight.
///
/// Being in view means an ordinary visit fetches OSM tiles, which the
/// collapsed panel this replaced deliberately avoided: that is what having
/// the map on screen costs, and it is paid on purpose.
#[component]
pub fn RegionFilter(filters: Signal<Filters>, mapped: Option<Mapped>) -> Element {
    // Where the map was last looking once it settled (US-73), and so the
    // region "Filter to map" takes (US-92).
    let viewport = use_signal(|| None::<Viewport>);
    // The activities of the lines drawn while zoomed in (US-73); `None`
    // while the map shows the marks, whose activities the legend names.
    let lined = use_signal(|| None::<Vec<ActivityType>>);
    // Whether waypoints are shown with the lines (US-90).
    let waypointed = use_signal(|| false);
    let shown = legend_activities(lined(), mapped.as_ref().map(|mapped| &mapped.items[..]));
    rsx! {
        section { class: "region",
            RegionMap { filters, mapped, viewport, lined, waypointed }
            if let Some(shown) = shown {
                ActivityLegend { shown, waypoints: waypointed() }
            }
            p { class: "region-controls",
                FilterToMap { filters, viewport }
                // Handled by the map's script alone: fitting the view to the
                // marks it already holds needs nothing from Rust.
                button {
                    r#type: "button",
                    id: "region-fit",
                    "Fit to items"
                }
            }
        }
    }
}

/// What the legend names: the activities of the lines while zoomed in
/// (US-73), else of the trips that got a mark (US-75). `None` until the list
/// has been read.
fn legend_activities(
    lined: Option<Vec<ActivityType>>,
    trips: Option<&[MapItem]>,
) -> Option<Vec<ActivityType>> {
    lined.or_else(|| trips.map(|trips| heat::marks(trips).activities()))
}

/// The page a click on a line opens: the trip's, or the collection track's
/// (US-87).
fn route_for(source: MapSource, id: i64) -> Route {
    match source {
        MapSource::Trips => Route::TripDetail { id },
        MapSource::CollectionTracks => Route::CollectionTrack { id },
    }
}

/// Where that page is, for a Ctrl- or ⌘-click to open in a new tab
/// (US-114).
fn line_href(source: MapSource, id: i64) -> String {
    crate::page_href(&route_for(source, id))
}

/// Turns the map's view into the region filter, and back off (US-92). Ticked
/// exactly when the filters hold a region, so a region from the URL ticks
/// it and "Clear filters" unticks it; there is nothing to take until the
/// map has said where it is looking.
#[component]
fn FilterToMap(mut filters: Signal<Filters>, viewport: Signal<Option<Viewport>>) -> Element {
    rsx! {
        label {
            input {
                r#type: "checkbox",
                id: "region-follow",
                checked: !filters.read().bbox.is_empty(),
                disabled: viewport().is_none(),
                onchange: move |event| {
                    let region = match viewport() {
                        Some(view) if event.checked() => interop::bbox_param(region_of(&view)),
                        _ => String::new(),
                    };
                    filters.write().bbox = region;
                },
            }
            "Filter to map"
        }
    }
}

/// The region the visible area filters by (US-92), `[west, south, east,
/// north]`. A view across the antimeridian, or wider than the world, covers
/// every longitude: the API rejects a region that crosses it (ADR-0011).
fn region_of(view: &Viewport) -> [f64; 4] {
    let [west, south, east, north] = view.bounds;
    if west < -180.0 || east > 180.0 {
        [-180.0, south, 180.0, north]
    } else {
        view.bounds
    }
}

/// The `bbox` the filters take once the map settles on `view` (US-92):
/// `None` while filtering to the map is off, or when the region already is
/// that view, so a settle without a move does not re-query the list.
fn followed(bbox: &str, view: &Viewport) -> Option<String> {
    if bbox.is_empty() {
        return None;
    }
    let region = interop::bbox_param(region_of(view));
    (region != bbox).then_some(region)
}

/// The map itself: draws the matching trips or collection tracks — as
/// marks, or zoomed in as lines (US-73) — and reports where it is looking,
/// which the region follows while filtering to the map is on (US-92). A
/// click on a line opens that trip or collection track.
#[component]
fn RegionMap(
    filters: Signal<Filters>,
    mapped: Option<Mapped>,
    mut viewport: Signal<Option<Viewport>>,
    lined: Signal<Option<Vec<ActivityType>>>,
    waypointed: Signal<bool>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut handle = use_signal(|| None::<document::Eval>);
    // Every track read while the list is open (US-73), and those asked for
    // whose answer is still on its way — one of each per source, since a
    // trip and a collection track may share an id. Two signals, not one: the
    // effect below redraws when tracks arrive, and only peeks at what is
    // pending, so a request that fails is not sent again until the view or
    // the list changes.
    let trip_cache = use_signal(TrackCache::default);
    let trip_pending = use_signal(HashSet::<i64>::new);
    let collection_cache = use_signal(TrackCache::default);
    let collection_pending = use_signal(HashSet::<i64>::new);
    // The same for the collections' waypoints shown with the lines (US-90).
    let waypoint_cache = use_signal(WaypointCache::default);
    let waypoint_pending = use_signal(HashSet::<i64>::new);
    let tracks_of = move |source| match source {
        MapSource::Trips => (trip_cache, trip_pending),
        MapSource::CollectionTracks => (collection_cache, collection_pending),
    };
    // Which lines are drawn, so lines the list no longer holds are not left
    // up while others load.
    let mut drawn = use_signal(|| None::<DrawnLines>);
    // One channel for the life of this component. `use_future` runs once, so
    // the re-render each new region causes — the filters change, the list
    // re-queries — does not restart the map or drop the channel
    // (`docs/eval-two-way-spike.md`).
    use_future(move || async move {
        let restore = interop::bbox_corners(&filters.peek().bbox);
        let mut map = interop::start_region_map(restore);
        handle.set(Some(map));
        loop {
            match map.recv::<RegionEvent>().await {
                Ok(RegionEvent::View(settled)) => {
                    viewport.set(Some(settled));
                    let region = followed(&filters.peek().bbox, &settled);
                    if let Some(region) = region {
                        filters.write().bbox = region;
                    }
                }
                Ok(RegionEvent::Open(line)) => {
                    navigator().push(route_for(line.source, line.id));
                }
                Ok(RegionEvent::OpenWaypoint(id)) => {
                    navigator().push(Route::Waypoint { id });
                }
                Err(err) => {
                    dioxus::logger::tracing::error!("the region map stopped reporting: {err}");
                    break;
                }
            }
        }
    });

    // Redrawn whenever the list's rows change — on the same terms the table
    // re-queries — whenever the view settles, and once more when the map
    // comes up, which may be after the first rows have arrived. Zoomed in,
    // the trips in view are drawn as their tracks once every one of them is
    // known; until then the map keeps what it shows, unless that is lines the
    // list no longer holds — those give way to the marks. Tracks not asked for
    // yet are fetched in one request that runs to its end whatever the view
    // does meanwhile, and what it brings redraws the map — so zooming while
    // tracks load waits for them instead of asking again.
    use_effect(use_reactive!(|mapped| {
        let (Some(map), Some(mapped)) = (handle(), mapped) else {
            return;
        };
        let items = &mapped.items;
        let marks = heat::marks(items).also_fitting(&mapped.also_fit);
        match viewport() {
            Some(view) if trip_lines::shows_lines(&view) => {
                let in_view = trip_lines::in_view(items, &view);
                // Each source's tracks not read yet, asked for in one
                // request of its own.
                let mut waiting = false;
                for source in SOURCES {
                    let (cache, mut pending) = tracks_of(source);
                    let ids: Vec<i64> = in_view
                        .iter()
                        .filter(|item| item.source() == source)
                        .map(|item| item.id)
                        .collect();
                    let missing = cache.read().missing(&ids);
                    if missing.is_empty() {
                        continue;
                    }
                    waiting = true;
                    let request = trip_lines::to_request(&missing, &pending.peek());
                    if !request.is_empty() {
                        pending.write().extend(&request);
                        spawn(fetch_tracks(archive(), source, cache, pending, request));
                    }
                }
                // The waypoints in view are asked for alongside, but hold no
                // line back (US-90): they join the lines once they arrive,
                // and are left off if they cannot be read.
                let collections = trip_lines::waypoints_in_view(&mapped.waypointed, &view);
                let missing = waypoint_cache.read().missing(&collections);
                let mut waypoints_pending = waypoint_pending;
                let request = trip_lines::to_request(&missing, &waypoints_pending.peek());
                if !request.is_empty() {
                    waypoints_pending.write().extend(&request);
                    spawn(fetch_waypoints(
                        archive(),
                        waypoint_cache,
                        waypoints_pending,
                        request,
                    ));
                }
                if waiting {
                    if outlived(drawn.peek().as_ref(), &mapped) {
                        drawn.set(None);
                        lined.set(None);
                        waypointed.set(false);
                        interop::draw_heat_marks(&map, &marks);
                    }
                    return;
                }
                let (trip_tracks, collection_tracks) = (trip_cache.read(), collection_cache.read());
                let tracks: Vec<_> = in_view
                    .iter()
                    .filter_map(|item| {
                        let cache = match item.source() {
                            MapSource::Trips => &trip_tracks,
                            MapSource::CollectionTracks => &collection_tracks,
                        };
                        Some((item.source(), cache.get(item.id)?))
                    })
                    .collect();
                let lines = trip_lines::lines(items, &tracks, &marks)
                    .with_waypoints(waypoint_cache.read().waypoints(&collections))
                    .with_pages(line_href);
                lined.set(Some(trip_lines::activities(items, &lines)));
                waypointed.set(!lines.waypoints.is_empty());
                drawn.set(Some(DrawnLines {
                    keys: lines
                        .lines
                        .iter()
                        .map(|line| (line.source, line.id))
                        .collect(),
                }));
                interop::draw_trip_lines(&map, &lines);
            }
            _ => {
                drawn.set(None);
                lined.set(None);
                waypointed.set(false);
                interop::draw_heat_marks(&map, &marks);
            }
        }
    }));

    // The sign shows while lines wait on the archive, not while marks do.
    let busy = viewport().is_some_and(|view| trip_lines::shows_lines(&view))
        && (SOURCES
            .into_iter()
            .any(|source| !tracks_of(source).1.read().is_empty())
            || !waypoint_pending.read().is_empty());

    rsx! {
        div { class: "region-map-frame",
            // Rendered empty and never given children: Leaflet owns this
            // subtree from the moment it initialises (ADR-0025).
            div { id: "region-map", class: "region-map" }
            if busy {
                MapBusy {}
            }
        }
    }
}

/// Where the map reads tracks from (US-87, US-90).
const SOURCES: [MapSource; 2] = [MapSource::Trips, MapSource::CollectionTracks];

/// The lines on the map: whose they are.
#[derive(Debug, Clone, PartialEq)]
struct DrawnLines {
    keys: Vec<(MapSource, i64)>,
}

/// Whether the lines drawn are of something `mapped` no longer holds — a
/// kind switched off, or a trip the list has dropped — so they should not
/// stay up while the tracks now in view load.
fn outlived(drawn: Option<&DrawnLines>, mapped: &Mapped) -> bool {
    drawn.is_some_and(|drawn| {
        drawn
            .keys
            .iter()
            .any(|key| !mapped.items.iter().any(|item| item.key() == *key))
    })
}

/// Fetch the tracks of `ids` into `cache` (US-73), from where `source`'s
/// are kept. A track that cannot be read is absent from the answer, and its
/// line left off the map, as on a share's. A request that fails outright
/// stores nothing, so the next view that needs those tracks asks again.
async fn fetch_tracks(
    archive: ApiClient,
    source: MapSource,
    mut cache: Signal<TrackCache>,
    mut pending: Signal<HashSet<i64>>,
    ids: Vec<i64>,
) {
    let answer = match source {
        MapSource::Trips => api::list_tracks(&archive, &ids).await,
        MapSource::CollectionTracks => api::list_collection_track_positions(&archive, &ids).await,
    };
    match answer {
        Ok(answer) => cache.write().store(&ids, answer),
        Err(err) => dioxus::logger::tracing::error!("could not read the tracks in view: {err}"),
    }
    pending.write().retain(|id| !ids.contains(id));
}

/// Fetch the waypoints of the collections of `ids` into `cache` (US-90), in
/// one request. A request that fails stores nothing, so the next view that
/// needs them asks again.
async fn fetch_waypoints(
    archive: ApiClient,
    mut cache: Signal<WaypointCache>,
    mut pending: Signal<HashSet<i64>>,
    ids: Vec<i64>,
) {
    match api::list_waypoints_of_collections(&archive, &ids).await {
        Ok(answer) => cache.write().store(&ids, answer),
        Err(err) => {
            dioxus::logger::tracing::error!("could not read the waypoints in view: {err}")
        }
    }
    pending.write().retain(|id| !ids.contains(id));
}

/// Over the map while the tracks in view load (US-73): zooming in past the
/// threshold waits on the archive, and nothing on the map shows it yet.
#[component]
fn MapBusy() -> Element {
    rsx! {
        div { id: "region-map-busy", class: "map-busy", role: "status",
            span { class: "spinner", "aria-hidden": "true" }
            "Loading tracks…"
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

// Split into region/tests.rs to keep this file under the repo's 500-line cap.
#[cfg(test)]
mod tests;
