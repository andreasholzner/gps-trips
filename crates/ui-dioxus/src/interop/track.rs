//! The detail screen's two widgets (US-7): the track on an OSM map, and the
//! elevation profile. Both are handed values Rust has already prepared
//! (`crate::track`) — these scripts only draw.

use dioxus::prelude::*;
use serde::Serialize;

use crate::photos::PhotoMarker;

/// The track map. Draws into `#track-map`: the polyline it is given, fitted
/// to the view, over OSM raster tiles.
///
/// The tile layer goes up before the polyline arrives, so a payload that is
/// slow or never comes leaves a usable map rather than a blank box — the
/// ordering the region map's spike arrived at (`docs/eval-two-way-spike.md`).
/// A view is set for the same reason: Leaflet refuses to render tiles until
/// it has one, and only the track can say where it should be.
///
/// Unlike the region map, this one is **payload-driven and redrawn**: the
/// same screen shows a different trip when the router swaps `id`. So the
/// instance is kept in a registry rather than the container being flagged as
/// done — a flag would make the second draw a no-op and leave the previous
/// trip's line on the map. The registry doubles as the guard ADR-0025 asks
/// for (a second script reuses the map instead of building another into the
/// same node) and as the disposal point: a container that was unmounted and
/// rebuilt leaves its map orphaned, holding Leaflet's window listeners, and
/// that one is removed rather than left to accumulate.
const TRACK_MAP_SCRIPT: &str = r##"
    const CONTAINER = "track-map";
    // The whole world, until the track says where it actually is.
    const EVERYWHERE = [[-60.0, -170.0], [75.0, 170.0]];

    async function ready() {
      for (let i = 0; i < 400; i++) {
        if (window.L && document.getElementById(CONTAINER)) return true;
        await new Promise((r) => setTimeout(r, 25));
      }
      return false;
    }
    if (!(await ready())) return;

    const el = document.getElementById(CONTAINER);
    const widgets = (window.tripArchiveWidgets ||= {});

    let map = widgets[CONTAINER];
    if (map && map.getContainer() !== el) {
      map.remove();
      map = null;
    }
    if (!map) {
      map = L.map(el);
      L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
        maxZoom: 19,
        attribution: "© OpenStreetMap contributors",
      }).addTo(map);
      map.fitBounds(EVERYWHERE);
      widgets[CONTAINER] = map;
    }

    // One payload carries the whole picture — the line and the photos on it —
    // so a redraw replaces what is there rather than layering on top of it.
    const view = await dioxus.recv();
    for (const layer of [map.trackLine, map.photoMarkers]) {
      if (layer) map.removeLayer(layer);
    }
    map.trackLine = null;
    map.photoMarkers = null;

    // A feature group, not a plain layer group: the view has to be able to
    // include a photo whose EXIF GPS puts it off the track (US-3), which
    // needs the markers' own bounds.
    map.photoMarkers = L.featureGroup(
      (view.markers || []).map((photo) => {
        const img = document.createElement("img");
        img.src = photo.thumbnail_url;
        img.alt = photo.name;
        img.style.maxWidth = "150px";
        return L.marker([photo.lat, photo.lon]).bindPopup(img);
      }),
    ).addTo(map);

    const points = view.points || [];
    if (points.length > 0) {
      map.trackLine = L.polyline(points, { color: "#3367d6", weight: 3 }).addTo(map);
    }

    // Framed on everything there is to see. A track with no drawable
    // positions still has photos, and they should not be left somewhere on a
    // view of the whole world.
    const bounds = L.latLngBounds([]);
    if (map.trackLine) bounds.extend(map.trackLine.getBounds());
    if (map.photoMarkers.getLayers().length > 0) {
      bounds.extend(map.photoMarkers.getBounds());
    }
    if (!bounds.isValid()) return;
    // Only when there is something new to frame. This script runs again
    // whenever the photos change, and refitting then would throw away a
    // zoom the owner had just made into part of the track.
    const framed = bounds.toBBoxString();
    if (map.framed !== framed) {
      map.fitBounds(bounds);
      map.framed = framed;
    }
"##;

/// The elevation profile. Draws into `#elevation`: elevation in metres
/// against cumulative distance in kilometres, the pair of series Rust
/// prepared.
///
/// Unlike the map there is nothing to show before the payload arrives — an
/// empty chart frame is not a useful thing to look at — so this one waits
/// for its series first and draws then.
///
/// Redrawn and disposed of on the same terms as the map above: a chart is
/// replaced outright rather than updated in place, because a new trip's
/// series are a different length and uPlot's axes have to be rebuilt anyway.
const ELEVATION_SCRIPT: &str = r##"
    const CONTAINER = "elevation";

    async function ready() {
      for (let i = 0; i < 400; i++) {
        if (window.uPlot && document.getElementById(CONTAINER)) return true;
        await new Promise((r) => setTimeout(r, 25));
      }
      return false;
    }
    if (!(await ready())) return;

    const el = document.getElementById(CONTAINER);
    const widgets = (window.tripArchiveWidgets ||= {});

    const [distanceKm, elevationM] = await dioxus.recv();
    if (widgets[CONTAINER]) {
      widgets[CONTAINER].destroy();
      widgets[CONTAINER] = null;
    }
    if (!distanceKm || distanceKm.length === 0) return;

    widgets[CONTAINER] = new uPlot(
      {
        width: el.clientWidth || 600,
        height: 200,
        scales: { x: { time: false } },
        series: [
          { label: "Distance (km)" },
          { label: "Elevation (m)", stroke: "#3367d6", width: 2 },
        ],
        axes: [{ label: "Distance (km)" }, { label: "Elevation (m)" }],
      },
      [distanceKm, elevationM],
      el,
    );
"##;

/// What the map shows: the track as `[lat, lon]` pairs, and a marker per
/// photo that has a position (US-3/US-4). Sent as one value because it is one
/// picture — a redraw with new markers must not lose the line.
#[derive(Serialize)]
struct TrackMapView {
    points: Vec<[f64; 2]>,
    markers: Vec<PhotoMarker>,
}

/// Start the track map with the line and the photo markers to draw.
///
/// The returned handle is the channel: it must be kept alive until the
/// script has taken the payload, so callers hold it for the life of the
/// screen.
pub fn start_track_map(points: Vec<[f64; 2]>, markers: Vec<PhotoMarker>) -> document::Eval {
    start(
        TRACK_MAP_SCRIPT,
        TrackMapView { points, markers },
        "the track map",
    )
}

/// Start the elevation chart with its two prepared series. The handle is the
/// channel, as above.
pub fn start_elevation_chart(distance_km: Vec<f64>, elevation_m: Vec<f64>) -> document::Eval {
    start(
        ELEVATION_SCRIPT,
        (distance_km, elevation_m),
        "the elevation chart",
    )
}

/// Run a drawing script and hand it its payload over the channel — never
/// spliced into the script text, which is the injection bug the whole
/// mechanism is shaped to make impossible (ADR-0025).
fn start(script: &str, payload: impl serde::Serialize, what: &str) -> document::Eval {
    let eval = document::eval(script);
    if let Err(err) = eval.send(payload) {
        dioxus::logger::tracing::error!("could not draw {what}: {err}");
    }
    eval
}
