//! The two JS widgets the detail screen needs (ADR-0005 Leaflet, ADR-0006
//! uPlot), driven through `document::eval`.
//!
//! **Why `eval` and not typed `wasm-bindgen` bindings:** the first version of
//! this spike drove uPlot through `#[wasm_bindgen] extern "C"` declarations,
//! which are compile-time checked and read like Rust. They also only exist on
//! `wasm32`. On Android the Rust runs natively inside a webview host, so
//! there is no wasm boundary to bind across and the typed style has no
//! meaning at all — `eval` is the only interop that works on both. That is
//! the single biggest thing the Android extension changed about this crate;
//! see the write-up for what was lost.
//!
//! Both functions take the container's DOM id rather than a node handle:
//! whichever element they draw into must be one Dioxus never re-renders, or
//! the virtual DOM and the JS library end up fighting over the same children.

use dioxus::prelude::*;

/// Waits for a JS global to appear, for scripts that are injected rather than
/// hardcoded into the page.
///
/// The libraries arrive as `document::Script` tags, which load asynchronously,
/// so neither `L` nor `uPlot` is guaranteed to exist by the time a track
/// resolves and its widget is drawn. Racing that would produce a map that
/// fails to appear on a fast network and works on a slow one — the worst kind
/// of bug to chase on a phone.
pub fn install_ready_helper() {
    document::eval(
        r#"
        window.whenReady = async (test, timeoutMs = 10000) => {
          const deadline = Date.now() + timeoutMs;
          while (!test()) {
            if (Date.now() > deadline) throw new Error("timed out waiting for a library to load");
            await new Promise((resolve) => setTimeout(resolve, 25));
          }
        };
        "#,
    );
}

/// Render the track on an OSM map (ADR-0005), fitted to its bounds.
///
/// The GeoJSON is handed over through `dioxus.recv()` rather than
/// interpolated into the script text — string-splicing a server value into
/// JS source is exactly the injection bug the server-rendered pages avoid
/// with `html_escape`.
pub fn draw_map(container_id: &str, track: serde_json::Value) {
    let eval = document::eval(
        r#"
        const [containerId, track] = await dioxus.recv();
        await whenReady(() => typeof L !== "undefined");
        const container = document.getElementById(containerId);
        if (!container || container.dataset.mapReady) return;
        container.dataset.mapReady = "1";

        const map = L.map(container);
        L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
          maxZoom: 19,
          attribution: "© OpenStreetMap contributors",
        }).addTo(map);

        const line = L.geoJSON(track).addTo(map);
        const bounds = line.getBounds();
        if (bounds.isValid()) map.fitBounds(bounds);
        "#,
    );
    if let Err(err) = eval.send((container_id.to_string(), track)) {
        dioxus::logger::tracing::error!("failed to hand the track to Leaflet: {err}");
    }
}

/// Plot elevation against cumulative distance (ADR-0006), from the parallel
/// arrays `build_track_geojson` stores in the track's `properties`.
///
/// The series are computed in Rust and sent over as plain numbers, so the
/// script stays a thin `new uPlot(...)` call. A track whose two series don't
/// line up (or is empty) is skipped rather than drawn half-right — the same
/// guard the server-rendered page applies.
pub fn draw_elevation(container_id: &str, track: &serde_json::Value) {
    let properties = &track["properties"];
    let distance_km: Vec<f64> = number_array(&properties["cumulative_distance_m"])
        .iter()
        .map(|metres| metres / 1000.0)
        .collect();
    let elevation = number_array(&properties["elevation_m"]);
    if distance_km.is_empty() || distance_km.len() != elevation.len() {
        return;
    }

    let eval = document::eval(
        r##"
        const [containerId, distanceKm, elevation] = await dioxus.recv();
        await whenReady(() => typeof uPlot !== "undefined");
        const container = document.getElementById(containerId);
        // An already-drawn chart is left alone: re-running would stack a
        // second canvas on top of the first.
        if (!container || container.childElementCount > 0) return;

        new uPlot({
          width: Math.max(container.clientWidth, 320),
          height: 200,
          scales: { x: { time: false } },
          series: [
            { label: "Distance (km)" },
            { label: "Elevation (m)", stroke: "#3367d6", width: 2 },
          ],
          axes: [{ label: "Distance (km)" }, { label: "Elevation (m)" }],
        }, [distanceKm, elevation], container);
        "##,
    );
    if let Err(err) = eval.send((container_id.to_string(), distance_km, elevation)) {
        dioxus::logger::tracing::error!("failed to hand the elevation series to uPlot: {err}");
    }
}

fn number_array(value: &serde_json::Value) -> Vec<f64> {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_f64)
                .collect()
        })
        .unwrap_or_default()
}
