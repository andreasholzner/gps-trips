//! The region map (US-52, carrying US-14; made the screen's centre by
//! US-63): the trips the filters match, drawn as a heat map — or, zoomed
//! in, as their tracks (US-73) — and the rectangle the owner drags to
//! narrow the list to trips whose stored bounding box overlaps it.
//!
//! The map itself is Leaflet, reached through `interop`; this module is the
//! Rust half — what to hand the map, and what to do with the rectangle it
//! reports back.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, TripSummary};

use crate::activity_color::ActivityLegend;
use crate::api::{self, ApiClient};
use crate::filters::Filters;
use crate::heat;
use crate::interop::{self, RegionEvent};
use crate::trip_lines::{self, Viewport};
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
    let armed = use_signal(|| false);
    // The activities of the lines drawn while zoomed in (US-73); `None`
    // while the map shows the marks, whose activities the legend names.
    let lined = use_signal(|| None::<Vec<ActivityType>>);
    let shown = legend_activities(lined(), trips.as_deref());
    rsx! {
        section { class: "region",
            RegionMap { filters, trips, armed, lined }
            if let Some(shown) = shown {
                ActivityLegend { shown }
            }
            p { class: "region-controls",
                SelectArea { armed }
                button {
                    r#type: "button",
                    id: "region-clear",
                    onclick: move |_| filters.write().bbox = String::new(),
                    "Clear region"
                }
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

/// Arms the map for drawing, or backs out of it (US-65). While armed a drag
/// draws rather than pans — by finger as much as by mouse — so the button
/// says so, and pressing it again is the way back. Drawing a rectangle
/// disarms it too.
#[component]
fn SelectArea(armed: Signal<bool>) -> Element {
    rsx! {
        button {
            r#type: "button",
            id: "region-select",
            aria_pressed: "{armed}",
            onclick: move |_| armed.toggle(),
            if armed() { "Cancel selection" } else { "Select area" }
        }
    }
}

/// The map itself: draws the rectangle the filters already hold and the
/// matching trips — as marks, or zoomed in as lines (US-73) — and writes
/// back every rectangle the owner drags while `armed`. A click on a line
/// opens that trip.
#[component]
fn RegionMap(
    filters: Signal<Filters>,
    trips: Option<Vec<TripSummary>>,
    armed: Signal<bool>,
    lined: Signal<Option<Vec<ActivityType>>>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut handle = use_signal(|| None::<document::Eval>);
    // Where the map was last looking once it settled (US-73).
    let mut viewport = use_signal(|| None::<Viewport>);
    // Whether the tracks in view are on their way (US-73).
    let mut busy = use_signal(|| false);
    // One channel for the life of this component. `use_future` runs once, so
    // the re-render each new rectangle causes — the filters change, the list
    // re-queries — does not restart the map or drop the channel
    // (`docs/eval-two-way-spike.md`).
    use_future(move || async move {
        let restore = interop::bbox_corners(&filters.peek().bbox);
        let mut map = interop::start_region_map(restore);
        handle.set(Some(map));
        loop {
            match map.recv::<RegionEvent>().await {
                Ok(RegionEvent::Region(corners)) => {
                    filters.write().bbox = interop::bbox_param(corners);
                    armed.set(false);
                }
                Ok(RegionEvent::View(settled)) => viewport.set(Some(settled)),
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

    // The rectangle follows the region the filters hold, so clearing it —
    // "Clear region", or "Clear filters" on the toolbar — takes it off the
    // map. A memo, so a keystroke in the search box sends nothing here.
    let region = use_memo(move || filters.read().bbox.clone());
    use_effect(move || {
        let corners = interop::bbox_corners(&region.read());
        if let Some(map) = handle.read().as_ref() {
            interop::show_region(map, corners);
        }
    });

    // Rust holds whether drawing is armed; the map only follows (ADR-0025).
    use_effect(move || {
        let armed = armed();
        if let Some(map) = handle.read().as_ref() {
            interop::arm_region_map(map, armed);
        }
    });

    // Redrawn whenever the list's rows change — on the same terms the table
    // re-queries — whenever the view settles, and once more when the map
    // comes up, which may be after the first rows have arrived. Zoomed in,
    // the trips in view are drawn as their tracks, fetched each time in one
    // request; a restart drops a fetch still under way, so an older view's
    // lines never land over a newer one's.
    let _draw = use_resource(use_reactive!(|trips| async move {
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
                // A track that cannot be read is absent from the answer, and
                // its line left off the map, as on a share's. A request that
                // fails outright leaves the map as it was.
                busy.set(!ids.is_empty());
                let tracks = api::list_tracks(&archive(), &ids).await;
                busy.set(false);
                let tracks = match tracks {
                    Ok(tracks) => tracks,
                    Err(err) => {
                        dioxus::logger::tracing::error!("could not read the tracks in view: {err}");
                        return;
                    }
                };
                let lines = trip_lines::lines(&trips, &tracks, &marks);
                lined.set(Some(trip_lines::activities(&trips, &lines)));
                interop::draw_trip_lines(&map, &lines);
            }
            _ => {
                busy.set(false);
                lined.set(None);
                interop::draw_heat_marks(&map, &marks);
            }
        }
    }));

    rsx! {
        div { class: "region-map-frame",
            // Rendered empty and never given children: Leaflet owns this
            // subtree from the moment it initialises (ADR-0025).
            div { id: "region-map", class: "region-map" }
            if busy() {
                MapBusy {}
            }
        }
    }
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
        for control in ["Select area", "Clear region", "Fit to trips"] {
            assert!(html.contains(control), "{control}: {html}");
        }
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

    #[test]
    fn select_area_says_when_it_is_armed_and_offers_a_way_back() {
        // US-65: once armed, a drag draws instead of panning, so the owner
        // must be able to see that and back out of it.
        let idle = render(|| rsx! { SelectArea { armed: Signal::new(false) } });
        let armed = render(|| rsx! { SelectArea { armed: Signal::new(true) } });

        assert!(idle.contains("Select area"), "{idle}");
        assert!(idle.contains(r#"aria-pressed="false""#), "{idle}");
        assert!(armed.contains("Cancel selection"), "{armed}");
        assert!(armed.contains(r#"aria-pressed="true""#), "{armed}");
    }
}
