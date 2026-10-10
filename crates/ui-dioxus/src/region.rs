//! The region map (US-52, carrying US-14; made the screen's centre by
//! US-63): the trips the filters match, drawn as a heat map — or, zoomed
//! in, as their tracks (US-73). With "Filter to map" ticked, its visible
//! area is the region (US-92): the list narrows to trips whose stored
//! bounding box overlaps it, and follows every pan and zoom.
//!
//! The map itself is Leaflet, reached through `interop`; this module is the
//! Rust half — what to hand the map, and what to do with the view it
//! reports back.

use std::collections::HashSet;

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, TripSummary};

use crate::activity_color::ActivityLegend;
use crate::api::{self, ApiClient};
use crate::filters::Filters;
use crate::heat;
use crate::interop::{self, RegionEvent};
use crate::trip_lines::{self, TrackCache, Viewport};
use crate::Route;

/// The map and its controls, always in view (US-63). `trips` — every trip
/// the filters match — is `None` until the list has been read, so the map
/// is not wiped blank while a re-query is in flight.
///
/// Being in view means an ordinary visit fetches OSM tiles, which the
/// collapsed panel this replaced deliberately avoided: that is what having
/// the map on screen costs, and it is paid on purpose.
#[component]
pub fn RegionFilter(filters: Signal<Filters>, trips: Option<Vec<TripSummary>>) -> Element {
    // Where the map was last looking once it settled (US-73), and so the
    // region "Filter to map" takes (US-92).
    let viewport = use_signal(|| None::<Viewport>);
    // The activities of the lines drawn while zoomed in (US-73); `None`
    // while the map shows the marks, whose activities the legend names.
    let lined = use_signal(|| None::<Vec<ActivityType>>);
    let shown = legend_activities(lined(), trips.as_deref());
    rsx! {
        section { class: "region",
            RegionMap { filters, trips, viewport, lined }
            if let Some(shown) = shown {
                ActivityLegend { shown }
            }
            p { class: "region-controls",
                FilterToMap { filters, viewport }
                // Handled by the map's script alone: fitting the view to the
                // marks it already holds needs nothing from Rust.
                button {
                    r#type: "button",
                    id: "region-fit",
                    "Fit to trips"
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
    trips: Option<&[TripSummary]>,
) -> Option<Vec<ActivityType>> {
    lined.or_else(|| trips.map(|trips| heat::marks(trips).activities()))
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

/// The map itself: draws the matching trips — as marks, or zoomed in as
/// lines (US-73) — and reports where it is looking, which the region
/// follows while filtering to the map is on (US-92). A click on a line opens
/// that trip.
#[component]
fn RegionMap(
    filters: Signal<Filters>,
    trips: Option<Vec<TripSummary>>,
    mut viewport: Signal<Option<Viewport>>,
    lined: Signal<Option<Vec<ActivityType>>>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut handle = use_signal(|| None::<document::Eval>);
    // Every track read while the list is open (US-73), and those asked for
    // whose answer is still on its way.
    let cache = use_signal(TrackCache::default);
    let mut pending = use_signal(HashSet::<i64>::new);
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
                Ok(RegionEvent::Open(id)) => {
                    navigator().push(Route::TripDetail { id });
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
    // known; until then the map keeps what it shows. Tracks not asked for
    // yet are fetched in one request that runs to its end whatever the view
    // does meanwhile, and what it brings redraws the map — so zooming while
    // tracks load waits for them instead of asking again.
    use_effect(use_reactive!(|trips| {
        let (Some(map), Some(trips)) = (handle(), trips) else {
            return;
        };
        let marks = heat::marks(&trips);
        match viewport() {
            Some(view) if trip_lines::shows_lines(&view) => {
                let ids: Vec<i64> = trip_lines::in_view(&trips, &view)
                    .iter()
                    .map(|trip| trip.id)
                    .collect();
                let missing = cache.read().missing(&ids);
                if !missing.is_empty() {
                    let request = trip_lines::to_request(&missing, &pending.peek());
                    if !request.is_empty() {
                        pending.write().extend(&request);
                        spawn(fetch_tracks(archive(), cache, pending, request));
                    }
                    return;
                }
                let lines = trip_lines::lines(&trips, &cache.read().tracks(&ids), &marks);
                lined.set(Some(trip_lines::activities(&trips, &lines)));
                interop::draw_trip_lines(&map, &lines);
            }
            _ => {
                lined.set(None);
                interop::draw_heat_marks(&map, &marks);
            }
        }
    }));

    // The sign shows while lines wait on the archive, not while marks do.
    let busy =
        viewport().is_some_and(|view| trip_lines::shows_lines(&view)) && !pending.read().is_empty();

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

/// Fetch the tracks of `ids` into `cache` (US-73). A track that cannot be
/// read is absent from the answer, and its line left off the map, as on a
/// share's. A request that fails outright stores nothing, so the next view
/// that needs those tracks asks again.
async fn fetch_tracks(
    archive: ApiClient,
    mut cache: Signal<TrackCache>,
    mut pending: Signal<HashSet<i64>>,
    ids: Vec<i64>,
) {
    let answer = api::list_tracks(&archive, &ids).await;
    match answer {
        Ok(answer) => cache.write().store(&ids, answer),
        Err(err) => dioxus::logger::tracing::error!("could not read the tracks in view: {err}"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use trip_archive_types::TripKind;

    /// A trip near Oslo, so it gets a mark.
    fn a_trip(activity_type: ActivityType) -> TripSummary {
        TripSummary {
            id: 1,
            name: "Trip".to_string(),
            activity_type,
            start_time: None,
            start_date: None,
            distance_m: 1_000.0,
            ascent_m: None,
            duration_secs: None,
            trip_kind: TripKind::Recorded,
            privacy_status: None,
            min_lat: Some(59.9),
            min_lon: Some(10.7),
            max_lat: Some(60.0),
            max_lon: Some(10.8),
        }
    }

    #[test]
    fn the_map_is_in_view_without_opening_anything() {
        // US-63: the map is the middle of the screen, not a filter's fine
        // print behind a disclosure.
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { RegionFilter { filters, trips: None } }
        });

        assert!(html.contains("region-map"), "{html}");
        assert!(!html.contains("<details"), "{html}");
        for control in ["Filter to map", "Fit to trips"] {
            assert!(html.contains(control), "{control}: {html}");
        }
    }

    #[test]
    fn us92_the_rectangle_and_its_buttons_are_gone() {
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { RegionFilter { filters, trips: None } }
        });

        for control in ["Select area", "Cancel selection", "Clear region"] {
            assert!(!html.contains(control), "{control}: {html}");
        }
    }

    #[test]
    fn us92_filtering_to_the_map_is_on_exactly_when_the_filters_hold_a_region() {
        // The checkbox has no state of its own: a region in the URL ticks
        // it, and "Clear filters" unticks it.
        let off = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { RegionFilter { filters, trips: None } }
        });
        let on = render(|| {
            let filters = Signal::new(Filters {
                bbox: "10.75,59.91,11.25,60.12".to_string(),
                ..Filters::default()
            });
            rsx! { RegionFilter { filters, trips: None } }
        });

        assert!(off.contains(r#"id="region-follow""#), "{off}");
        assert!(!off.contains("checked"), "{off}");
        assert!(on.contains("checked"), "{on}");
    }

    #[test]
    fn us92_filtering_to_the_map_waits_until_the_map_says_where_it_is() {
        // Before the first view is reported there is no region to take.
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { RegionFilter { filters, trips: None } }
        });

        assert!(html.contains("disabled"), "{html}");
    }

    fn view(bounds: [f64; 4]) -> Viewport {
        Viewport { zoom: 9.0, bounds }
    }

    #[test]
    fn us92_the_region_is_the_visible_area() {
        assert_eq!(
            region_of(&view([10.5, 59.8, 11.0, 60.0])),
            [10.5, 59.8, 11.0, 60.0]
        );
    }

    #[test]
    fn us92_a_view_across_the_antimeridian_covers_every_longitude() {
        // The API rejects a region that crosses it (ADR-0011).
        assert_eq!(
            region_of(&view([170.0, -20.0, 190.0, -10.0])),
            [-180.0, -20.0, 180.0, -10.0]
        );
    }

    #[test]
    fn us92_a_view_wider_than_the_world_covers_every_longitude() {
        assert_eq!(
            region_of(&view([-180.0, -60.0, 300.0, 80.0])),
            [-180.0, -60.0, 180.0, 80.0]
        );
        assert_eq!(
            region_of(&view([-250.0, -60.0, 100.0, 80.0])),
            [-180.0, -60.0, 180.0, 80.0]
        );
    }

    #[test]
    fn us92_with_filtering_off_the_view_narrows_nothing() {
        assert_eq!(followed("", &view([10.5, 59.8, 11.0, 60.0])), None);
    }

    #[test]
    fn us92_with_filtering_on_the_region_follows_the_view() {
        assert_eq!(
            followed("-30,30,-20,40", &view([10.5, 59.8, 11.0, 60.0])),
            Some("10.500000,59.800000,11.000000,60.000000".to_string())
        );
    }

    #[test]
    fn us92_a_view_that_did_not_move_changes_nothing() {
        // A settle without a move must not re-query the list.
        let bounds = [10.5, 59.8, 11.0, 60.0];
        let held = interop::bbox_param(bounds);

        assert_eq!(followed(&held, &view(bounds)), None);
    }

    #[test]
    fn the_map_names_the_colors_of_the_activities_it_marks() {
        // US-75.
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            let trips = vec![a_trip(ActivityType::Hiking), a_trip(ActivityType::Kayaking)];
            rsx! { RegionFilter { filters, trips: Some(trips) } }
        });

        assert!(html.contains("map-legend"), "{html}");
        assert!(
            html.contains("Hiking") && html.contains("Kayaking"),
            "{html}"
        );
    }

    #[test]
    fn zoomed_in_the_legend_names_the_activities_of_the_lines_drawn() {
        // US-73: a kayak trip marked elsewhere but with no line in view is
        // not on the map, so not in its legend.
        let trips = [a_trip(ActivityType::Hiking), a_trip(ActivityType::Kayaking)];

        assert_eq!(
            legend_activities(Some(vec![ActivityType::Hiking]), Some(&trips)),
            Some(vec![ActivityType::Hiking])
        );
        assert_eq!(
            legend_activities(None, Some(&trips)),
            Some(vec![ActivityType::Hiking, ActivityType::Kayaking])
        );
        assert_eq!(legend_activities(None, None), None);
    }

    #[test]
    fn the_map_is_not_busy_before_it_asks_for_anything() {
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { RegionFilter { filters, trips: None } }
        });

        assert!(!html.contains("region-map-busy"), "{html}");
    }

    #[test]
    fn while_the_tracks_load_the_map_says_so() {
        // US-73: switching to lines waits on the archive, and says so.
        let html = render(|| rsx! { MapBusy {} });

        assert!(html.contains(r#"role="status""#), "{html}");
        assert!(html.contains("Loading tracks…"), "{html}");
    }
}
