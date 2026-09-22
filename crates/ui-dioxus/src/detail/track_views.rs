//! The track on the detail screen (US-7): the map and the elevation profile,
//! and what the cursor reads out between them (US-59).

use dioxus::prelude::*;

use crate::api::{self, ApiClient};
use crate::format;
use crate::interop;
use crate::photos::PhotoMarker;
use crate::track::{self, Track};

/// The track on an OSM map and the elevation profile below it (US-7), from
/// the one fetch that carries both (ADR-0025).
///
/// A track that will not load costs the map and the chart, not the screen:
/// the stats, the gallery and the edit controls around it are unaffected.
#[component]
pub fn TrackSection(id: i64, markers: Vec<PhotoMarker>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let track = use_resource(use_reactive!(|id| async move {
        api::get_track(&archive(), id).await
    }));

    rsx! {
        match &*track.read_unchecked() {
            None => rsx! { p { "Loading the track…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the track: {err}" } },
            Some(Ok(track)) => rsx! {
                TrackViews { track: track.clone(), markers: markers.clone() }
            },
        }
    }
}

/// The two widgets themselves, once there is a track to draw. Split from the
/// fetch above so each starts its script exactly once, when it mounts with
/// the values it draws — never on a re-render of the screen around it.
///
/// This is also where the hover (US-59) turns around: the chart reports the
/// index the cursor is on, and the index is resolved *here*, in Rust, into
/// the position the map marks and the values the readout shows. The chart's
/// index and the polyline's are not the same point (`track::hover_points`),
/// which is the whole reason the resolution does not happen in either script.
#[component]
fn TrackViews(track: Track, markers: Vec<PhotoMarker>) -> Element {
    let points = track::polyline(&track);
    let series = track::elevation_series(&track);
    let samples = track::hover_points(&track);
    let hovered = use_signal(|| None::<usize>);
    let hovered_at = hovered().and_then(|i| samples.get(i).and_then(|sample| sample.position));

    rsx! {
        TrackMap { points, markers, hovered_at }
        if let Some((distance_km, elevation_m)) = series {
            ElevationChart { distance_km, elevation_m, hovered }
            HoverReadout { points: samples, hovered: hovered() }
        }
    }
}

/// The map container. Rendered empty and never given children: Leaflet owns
/// this subtree from the moment it initialises (ADR-0025).
#[component]
fn TrackMap(
    points: Vec<[f64; 2]>,
    markers: Vec<PhotoMarker>,
    hovered_at: Option<[f64; 2]>,
) -> Element {
    // The handle is the channel, so it is held for as long as the map should
    // keep taking messages — which is now the component's whole life, not
    // just until the first payload lands (US-59).
    let mut handle = use_signal(|| None::<document::Eval>);

    // Redrawn whenever the line changes, not only when the component first
    // mounts: the router shows a different trip through this same component,
    // and a plain `use_future` would leave the previous trip's track on the
    // map. The script is written to be drawn into twice (`interop::track`),
    // and replacing the handle here closes the superseded script's channel,
    // which is how that one learns to stop.
    use_future(use_reactive!(|points, markers| async move {
        handle.set(Some(interop::start_track_map(points, markers)));
    }));

    // Sent on every change of the hovered point, and again when a redraw
    // replaces the handle — a fresh map must not be left without the mark
    // the cursor is still asking for.
    use_effect(use_reactive!(|hovered_at| {
        if let Some(map) = handle.read().as_ref() {
            interop::mark_on_track_map(map, hovered_at);
        }
    }));

    rsx! { div { id: "track-map", class: "track-map" } }
}

/// The elevation chart's container, on the same terms as the map's — and the
/// source of the hovered index, which it reports until the channel closes.
#[component]
fn ElevationChart(
    distance_km: Vec<f64>,
    elevation_m: Vec<f64>,
    hovered: Signal<Option<usize>>,
) -> Element {
    use_future(use_reactive!(|distance_km, elevation_m| {
        let mut hovered = hovered;
        async move {
            let mut chart = interop::start_elevation_chart(distance_km, elevation_m);
            loop {
                match chart.recv::<Option<usize>>().await {
                    Ok(index) => hovered.set(index),
                    // The channel closed (a redraw, or the screen going away)
                    // or said something this end cannot read. Either way this
                    // chart reports nothing further, and the mark it last
                    // asked for is cleared rather than left behind.
                    Err(err) => {
                        dioxus::logger::tracing::debug!("the chart stopped reporting: {err}");
                        hovered.set(None);
                        break;
                    }
                }
            }
        }
    }));

    rsx! { div { id: "elevation", class: "elevation" } }
}

/// What the cursor is on, under the chart (US-59): the distance and elevation
/// of the hovered sample, through the same formatting as the stats above, so
/// the two read alike, and when it was there (US-62). Dashes when the cursor
/// is not on the chart — the same dash the stats use for a value there is
/// none of. A track with no times at all has no time to read out, and says
/// nothing about it rather than show a dash that can never fill.
///
/// This is uPlot's own legend, rebuilt as ordinary markup: the stock one is
/// the control that hides the series when clicked, and its marker square
/// reads as a checkbox.
#[component]
fn HoverReadout(points: Vec<track::HoverPoint>, hovered: Option<usize>) -> Element {
    let at = hovered.and_then(|index| points.get(index));
    let timed = track::has_times(&points);

    rsx! {
        p { id: "chart-readout", class: "chart-readout",
            "At cursor: "
            span { id: "readout-distance",
                {at.map_or_else(|| "—".to_string(), |sample| format::km(sample.distance_m))}
            }
            " · "
            span { id: "readout-elevation",
                {format::metres(at.map(|sample| sample.elevation_m))}
            }
            if timed {
                " · "
                span { id: "readout-time",
                    {format::or_dash(at.and_then(|sample| sample.time.as_deref()))}
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    // ── US-59: the readout under the chart ───────────────────────────────

    fn a_sample(distance_m: f64, elevation_m: f64) -> track::HoverPoint {
        track::HoverPoint {
            distance_m,
            elevation_m,
            position: Some([59.91, 10.75]),
            time: None,
        }
    }

    #[test]
    fn the_readout_claims_no_position_until_the_cursor_is_on_the_chart() {
        let points = vec![a_sample(0.0, 12.0), a_sample(1_234.0, 340.0)];

        let html = render(move || {
            rsx! { HoverReadout { points: points.clone(), hovered: None } }
        });

        assert!(html.contains("At cursor"), "{html}");
        // Dashes, and the same dash the stats use for a value there is none of.
        assert!(html.contains('—'), "{html}");
        assert!(!html.contains("1.23 km"), "{html}");
    }

    #[test]
    fn the_readout_shows_the_hovered_sample_the_way_the_stats_do() {
        // Through `format::km` and `format::metres`, so a distance under the
        // chart reads exactly like the distance above it (US-59).
        let points = vec![a_sample(0.0, 12.0), a_sample(1_234.0, 340.0)];

        let html = render(move || {
            rsx! { HoverReadout { points: points.clone(), hovered: Some(1) } }
        });

        assert!(html.contains("1.23 km"), "{html}");
        assert!(html.contains("340 m"), "{html}");
    }

    #[test]
    fn a_hovered_index_the_chart_no_longer_has_reads_as_nothing() {
        // The index and the series arrive on different paths: a chart drawn
        // for the previous trip can report an index this one is too short
        // for, and that must read as "no position" rather than panic.
        let points = vec![a_sample(0.0, 12.0)];

        let html = render(move || {
            rsx! { HoverReadout { points: points.clone(), hovered: Some(7) } }
        });

        assert!(html.contains('—'), "{html}");
    }

    // ── US-62: the time of the hovered point ─────────────────────────────

    fn a_timed_sample(time: Option<&str>) -> track::HoverPoint {
        track::HoverPoint {
            time: time.map(str::to_string),
            ..a_sample(0.0, 12.0)
        }
    }

    #[test]
    fn the_readout_says_when_the_hovered_point_was() {
        let points = vec![a_timed_sample(Some("14:32 (+02:00)"))];

        let html = render(move || {
            rsx! { HoverReadout { points: points.clone(), hovered: Some(0) } }
        });

        assert!(html.contains(r#"id="readout-time""#), "{html}");
        assert!(html.contains("14:32 (+02:00)"), "{html}");
    }

    #[test]
    fn a_point_the_gpx_gave_no_time_reads_as_a_dash() {
        let points = vec![a_timed_sample(Some("14:32 (+02:00)")), a_timed_sample(None)];

        let html = render(move || {
            rsx! { HoverReadout { points: points.clone(), hovered: Some(1) } }
        });

        assert!(html.contains(r#"id="readout-time">—<"#), "{html}");
    }

    #[test]
    fn a_track_with_no_times_drops_the_time_rather_than_show_a_dash_forever() {
        let points = vec![a_timed_sample(None), a_timed_sample(None)];

        let html = render(move || {
            rsx! { HoverReadout { points: points.clone(), hovered: Some(1) } }
        });

        assert!(!html.contains("readout-time"), "{html}");
    }
}
