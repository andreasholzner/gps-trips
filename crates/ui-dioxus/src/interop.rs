//! The two JS widgets the detail screen needs (ADR-0005 Leaflet, ADR-0006
//! uPlot), deliberately driven through **two different interop styles** so
//! the spike can compare them:
//!
//! * [`draw_map`] uses `document::eval` — a JS source string plus a channel
//!   for the payload. No bindings to write, no build-time coupling to the
//!   library's shape; in exchange, nothing about the script is type-checked
//!   and every mistake surfaces only in the browser console.
//! * [`draw_elevation`] uses `wasm-bindgen` externs — uPlot's constructor is
//!   declared once as a typed Rust signature, and the options struct is a
//!   real `#[derive(Serialize)]` type. Compile-time-checked call sites, at
//!   the cost of a binding per JS entry point.
//!
//! Both take the container's DOM id rather than a node handle: whichever
//! element they draw into must be one Dioxus itself never re-renders, or the
//! virtual DOM and the JS library end up fighting over the same children.

use dioxus::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

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

/// uPlot's constructor: `new uPlot(options, data, targetElement)`.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = uPlot)]
    type UPlot;

    #[wasm_bindgen(constructor, js_class = "uPlot")]
    fn new(options: &JsValue, data: &JsValue, target: &web_sys::Element) -> UPlot;
}

#[derive(Serialize)]
struct ChartOptions {
    width: u32,
    height: u32,
    scales: Scales,
    series: Vec<Series>,
    axes: Vec<Axis>,
}

#[derive(Serialize)]
struct Scales {
    x: XScale,
}

#[derive(Serialize)]
struct XScale {
    /// uPlot treats the x-axis as timestamps unless told otherwise; ours is
    /// distance travelled.
    time: bool,
}

#[derive(Serialize)]
struct Series {
    label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    stroke: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
}

#[derive(Serialize)]
struct Axis {
    label: &'static str,
}

/// Plot elevation against cumulative distance (ADR-0006), from the parallel
/// arrays `build_track_geojson` stores in the track's `properties`.
///
/// A track whose two series don't line up (or is empty) is skipped rather
/// than drawn half-right — the same guard the server-rendered page applies.
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

    let Some(container) = element_by_id(container_id) else {
        return;
    };
    // Leaving an already-drawn chart alone keeps a re-render from stacking a
    // second canvas on top of the first.
    if container.child_element_count() > 0 {
        return;
    }

    let options = ChartOptions {
        width: container.client_width().max(320) as u32,
        height: 200,
        scales: Scales {
            x: XScale { time: false },
        },
        series: vec![
            Series {
                label: "Distance (km)",
                stroke: None,
                width: None,
            },
            Series {
                label: "Elevation (m)",
                stroke: Some("#3367d6"),
                width: Some(2),
            },
        ],
        axes: vec![
            Axis {
                label: "Distance (km)",
            },
            Axis {
                label: "Elevation (m)",
            },
        ],
    };

    // `Serializer::json_compatible` matters: serde-wasm-bindgen's default
    // turns every struct into a JS `Map`, which uPlot (like any plain-object
    // JS API) silently reads as an options object with no options in it.
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let (Ok(options), Ok(data)) = (
        options.serialize(&serializer),
        vec![distance_km, elevation].serialize(&serializer),
    ) else {
        dioxus::logger::tracing::error!("failed to convert the elevation series for uPlot");
        return;
    };

    UPlot::new(&options, &data, &container);
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

fn element_by_id(id: &str) -> Option<web_sys::Element> {
    web_sys::window()?.document()?.get_element_by_id(id)
}
