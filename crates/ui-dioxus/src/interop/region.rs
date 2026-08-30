//! The region map (US-52/US-14) — the trip list's own widget.

use dioxus::prelude::*;

/// Coordinate decimals kept in the `bbox` parameter. Six is ~10 cm — far
/// finer than a rectangle dragged by hand needs, and it keeps a shared URL
/// readable.
const BBOX_DECIMALS: usize = 6;

/// The region map (US-52/US-14). Draws into `#region-map`, restores the
/// rectangle it is given, and reports every finished drag back as four
/// numbers: `[west, south, east, north]`.
///
/// Coordinate hygiene stays on the JS side because it needs the live map:
/// Leaflet's world repeats horizontally, so a map panned east reports
/// longitudes like 309 rather than -51, and a drag into the blank space
/// above the projected world reports latitudes beyond ±90 — either of which
/// the server rejects with a 400. `wrapLatLngBounds` shifts the rectangle
/// back by whole world widths, keeping its size and its place on the globe.
/// A rectangle that genuinely straddles the antimeridian still ends up out
/// of range and still gets the server's 400: that one is unsupported in v1
/// (ADR-0011), unlike merely having panned east.
const REGION_MAP_SCRIPT: &str = r##"
    const CONTAINER = "region-map";
    // Europe, roughly Iceland to Malta and Iberia to the western Urals —
    // the default view when no region has been chosen yet.
    const EUROPE = [[34.0, -12.0], [66.0, 34.0]];

    async function ready() {
      for (let i = 0; i < 400; i++) {
        if (window.L && document.getElementById(CONTAINER)) return true;
        await new Promise((r) => setTimeout(r, 25));
      }
      return false;
    }
    if (!(await ready())) return;

    const el = document.getElementById(CONTAINER);
    // A JS library owns its subtree outright; never build a second map into
    // the same node.
    if (el.dataset.mapReady === "1") return;
    el.dataset.mapReady = "1";

    const map = L.map(el);
    L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
      maxZoom: 19,
      attribution: "© OpenStreetMap contributors",
    }).addTo(map);

    let rect = null;
    const show = (bounds) => {
      if (rect) rect.setBounds(bounds);
      else rect = L.rectangle(bounds, { color: "#3388ff", weight: 2 }).addTo(map);
    };

    map.fitBounds(EUROPE);

    // Drawing is armed by the owner, so an ordinary drag still pans the map.
    let drawing = false;
    let origin = null;
    document.getElementById("region-select")?.addEventListener("click", () => {
      drawing = true;
      map.dragging.disable();
    });

    map.on("mousedown", (e) => {
      if (!drawing) return;
      origin = e.latlng;
      show([origin, origin]);
    });
    map.on("mousemove", (e) => {
      if (!drawing || !origin) return;
      show([origin, e.latlng]);
    });
    // On the document, not the map: a mouseup outside the map never reaches
    // Leaflet, which would strand a rectangle the filters do not hold.
    document.addEventListener("mouseup", () => {
      if (!drawing || !origin) return;
      const w = map.wrapLatLngBounds(rect.getBounds());
      const clampLat = (n) => Math.min(90, Math.max(-90, n));
      origin = null;
      drawing = false;
      map.dragging.enable();
      dioxus.send([
        w.getWest(), clampLat(w.getSouth()),
        w.getEast(), clampLat(w.getNorth()),
      ]);
    });

    // Only now wait for the rectangle the filters already hold — after the
    // map is interactive, never before. Awaiting the channel first would
    // leave the map drawn but dead if that message were slow or never came,
    // which is exactly what it did.
    const restore = await dioxus.recv();
    if (restore && !rect) {
      const bounds = [[restore[1], restore[0]], [restore[3], restore[2]]];
      show(bounds);
      map.fitBounds(bounds, { padding: [20, 20] });
    }
"##;

/// Start the region map, handing it the rectangle the filters already hold.
///
/// The returned handle must be kept alive — and `recv()`ed in a loop — for
/// as long as the map should report drags: it is the channel.
pub fn start_region_map(restore: Option<[f64; 4]>) -> document::Eval {
    let eval = document::eval(REGION_MAP_SCRIPT);
    if let Err(err) = eval.send(restore) {
        dioxus::logger::tracing::error!("could not seed the region map: {err}");
    }
    eval
}

/// The four corners the map reported, as the `bbox` query parameter the API
/// takes (`minLon,minLat,maxLon,maxLat`, ADR-0008).
pub fn bbox_param(corners: [f64; 4]) -> String {
    corners
        .iter()
        .map(|n| format!("{n:.BBOX_DECIMALS$}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// The inverse, for restoring a stored rectangle onto the map. `None` for
/// anything that is not four numbers — a hand-edited URL loses the
/// rectangle rather than breaking the screen.
pub fn bbox_corners(param: &str) -> Option<[f64; 4]> {
    let numbers: Vec<f64> = param
        .split(',')
        .map(|part| part.trim().parse().ok())
        .collect::<Option<Vec<f64>>>()?;
    numbers.try_into().ok()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_become_the_apis_bbox_parameter() {
        assert_eq!(
            bbox_param([10.75, 59.91, 11.25, 60.12]),
            "10.750000,59.910000,11.250000,60.120000"
        );
    }

    #[test]
    fn coordinates_are_rounded_not_truncated_at_six_decimals() {
        // Six decimals is ~10 cm; the point is a readable URL, not precision
        // no hand-dragged rectangle has.
        assert_eq!(
            bbox_param([1.234_567_89, -2.000_000_4, 3.5, -4.0]),
            "1.234568,-2.000000,3.500000,-4.000000"
        );
    }

    #[test]
    fn a_stored_bbox_round_trips_back_into_corners() {
        let corners = [10.75, 59.91, 11.25, 60.12];

        assert_eq!(bbox_corners(&bbox_param(corners)), Some(corners));
    }

    #[test]
    fn a_malformed_bbox_restores_nothing_rather_than_breaking_the_screen() {
        assert_eq!(bbox_corners(""), None);
        assert_eq!(bbox_corners("10,20,30"), None);
        assert_eq!(bbox_corners("10,20,30,40,50"), None);
        assert_eq!(bbox_corners("10,20,thirty,40"), None);
    }
}
