//! The map a photo is placed on by hand (US-30): the track to find the spot
//! by, the photo's current position if it has one, and the point the owner
//! picks. Rust decides what a pick means (`placing::picked`); this only draws
//! and reports.

use dioxus::prelude::*;
use serde::Serialize;

/// Draws into `#place-map`, over OSM tiles: the track as a line, the photo's
/// current position as a hollow ring, and — once the owner has tapped or
/// clicked — a filled mark where the photo will go, moved by every later
/// tap. Each pick is sent back as `[lat, lon]`.
///
/// The map is built afresh for every placement: the overlay that holds it is
/// a new node each time it opens. A map left in the registry by an earlier
/// placement belongs to a container that is gone, and is removed rather than
/// left holding Leaflet's window listeners — the track map's disposal rule.
const PLACE_MAP_SCRIPT: &str = r##"
    const CONTAINER = "place-map";
    // The whole world, until the payload says where the trip is.
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
    if (widgets[CONTAINER]) {
      widgets[CONTAINER].remove();
      widgets[CONTAINER] = null;
    }
    const map = L.map(el);
    L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
      maxZoom: 19,
      attribution: "© OpenStreetMap contributors",
    }).addTo(map);
    map.fitBounds(EVERYWHERE);
    widgets[CONTAINER] = map;

    const view = await dioxus.recv();
    const bounds = L.latLngBounds([]);
    const points = view.points || [];
    if (points.length > 0) {
      const line = L.polyline(points, {
        color: "#3367d6",
        weight: 3,
        interactive: false,
      }).addTo(map);
      bounds.extend(line.getBounds());
    }
    if (view.current) {
      // Hollow, so it reads as "was here" beside the filled mark that says
      // "goes here"; not interactive, so a tap on it still picks that spot.
      L.circleMarker(view.current, {
        radius: 8,
        color: "#d6336c",
        weight: 3,
        fill: false,
        className: "place-current",
        interactive: false,
      }).addTo(map);
      bounds.extend(view.current);
    }
    if (bounds.isValid()) {
      map.fitBounds(bounds, { maxZoom: 16 });
    }

    // Leaflet's default pin is an image file neither the bundle nor the APK
    // ships, so the pick is a circle, like the track map's photo markers.
    let picked = null;
    map.on("click", (event) => {
      const at = [event.latlng.lat, event.latlng.lng];
      if (picked) {
        picked.setLatLng(at);
      } else {
        picked = L.circleMarker(at, {
          radius: 7,
          color: "#ffffff",
          weight: 2,
          fillColor: "#d6336c",
          fillOpacity: 1,
          className: "place-picked",
          interactive: false,
        }).addTo(map);
      }
      try {
        dioxus.send(at);
      } catch {
        // The overlay closed; nothing is listening for a pick any more.
      }
    });
"##;

/// What the placing map shows: the track as `[lat, lon]` pairs, and where the
/// photo is now, if anywhere.
#[derive(Serialize)]
struct PlaceMapView {
    points: Vec<[f64; 2]>,
    current: Option<[f64; 2]>,
}

/// Start the placing map. The handle is the channel, and is read for the
/// map's whole life: every pick arrives on it as `[lat, lon]`, exactly as
/// Leaflet reports it — unwrapped, so possibly off the ±180° range.
pub fn start_place_map(points: Vec<[f64; 2]>, current: Option<[f64; 2]>) -> document::Eval {
    let eval = document::eval(PLACE_MAP_SCRIPT);
    if let Err(err) = eval.send(PlaceMapView { points, current }) {
        dioxus::logger::tracing::error!("could not draw the placing map: {err}");
    }
    eval
}
