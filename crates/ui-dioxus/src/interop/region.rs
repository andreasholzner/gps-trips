//! The region map (US-52/US-14, US-63) — the trip list's own widget.

use dioxus::prelude::*;
use serde::Serialize;

use crate::heat::HeatMarks;

/// Coordinate decimals kept in the `bbox` parameter. Six is ~10 cm — far
/// finer than a rectangle dragged by hand needs, and it keeps a shared URL
/// readable.
const BBOX_DECIMALS: usize = 6;

/// The region map (US-52/US-14, US-63, US-65). Draws into `#region-map`,
/// restores the rectangle it is given, and reports every finished drag while
/// armed back as four numbers: `[west, south, east, north]`.
///
/// Its channel carries [`MapMessage`]s: the rectangle the filters hold —
/// first when the map starts, then whenever the region changes, including
/// to none when it is cleared — and the marks every time the list's rows
/// change. The view fits once — to the first rectangle, or else to the
/// first marks there are — and after that only when the owner asks with
/// "Fit to trips", so the map does not jump on every keystroke.
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

    // The rectangle the filters hold, put back when a drag comes to nothing.
    let held = null;
    const restore = () => {
      if (held) show(held);
      else {
        rect?.remove();
        rect = null;
      }
    };

    // Drawing is armed from Rust (US-65), so an ordinary drag still pans the
    // map. While armed, one pointer draws — mouse, finger or pen alike — and
    // everything else a touch could do to the map is off: panning, pinch
    // zoom and the two-finger pan that comes with it, and, through
    // `region-drawing`'s `touch-action: none`, the browser's own pinch of
    // the page.
    let drawing = false;
    let origin = null;
    let pointer = null;
    const arm = (on) => {
      drawing = on;
      if (!on && pointer !== null) {
        pointer = null;
        restore();
      }
      el.classList.toggle("region-drawing", on);
      for (const handler of [map.dragging, map.touchZoom]) {
        if (on) handler.disable();
        else handler.enable();
      }
    };

    // A tap, or a slip of the finger, is not a region: anything narrower or
    // lower than this on screen is dropped, and the map stays armed.
    const MIN_SIDE = 5;

    el.addEventListener("pointerdown", (e) => {
      if (!drawing || !e.isPrimary || pointer !== null) return;
      // The zoom buttons keep their clicks.
      if (e.target.closest(".leaflet-control")) return;
      e.preventDefault();
      pointer = e.pointerId;
      origin = map.mouseEventToContainerPoint(e);
      // Captured, so a pointer lifted outside the map still ends the drag
      // rather than stranding a rectangle the filters do not hold.
      el.setPointerCapture(pointer);
      const at = map.containerPointToLatLng(origin);
      show([at, at]);
    });
    el.addEventListener("pointermove", (e) => {
      if (e.pointerId !== pointer) return;
      show([map.containerPointToLatLng(origin), map.mouseEventToLatLng(e)]);
    });
    el.addEventListener("pointerup", (e) => {
      if (e.pointerId !== pointer) return;
      pointer = null;
      const end = map.mouseEventToContainerPoint(e);
      if (Math.abs(end.x - origin.x) < MIN_SIDE || Math.abs(end.y - origin.y) < MIN_SIDE) {
        restore();
        return;
      }
      const w = map.wrapLatLngBounds(L.latLngBounds(
        map.containerPointToLatLng(origin), map.containerPointToLatLng(end),
      ));
      const clampLat = (n) => Math.min(90, Math.max(-90, n));
      dioxus.send([
        w.getWest(), clampLat(w.getSouth()),
        w.getEast(), clampLat(w.getNorth()),
      ]);
    });
    // Taken away by the browser — a call coming in, say: not a region.
    el.addEventListener("pointercancel", (e) => {
      if (e.pointerId !== pointer) return;
      pointer = null;
      restore();
    });

    // Only now read the channel — after the map is interactive, never
    // before. Awaiting it first would leave the map drawn but dead if the
    // first message were slow or never came, which is exactly what it did.
    let fitted = false;

    // The heat marks (US-63): not interactive, so a drag that starts on one
    // still draws or pans. `heat-mark` names them for the browser tests.
    const heat = L.layerGroup().addTo(map);
    let points = [];
    const fitToMarks = () => {
      if (points.length === 0) return;
      map.fitBounds(L.latLngBounds(points), { padding: [20, 20], maxZoom: 12 });
    };
    document.getElementById("region-fit")?.addEventListener("click", fitToMarks);

    for (;;) {
      const message = await dioxus.recv();
      if ("region" in message) {
        // The rectangle follows the filters, so clearing the region —
        // by "Clear region" or by "Clear filters" — takes it off the map.
        const region = message.region;
        held = region && [[region[1], region[0]], [region[3], region[2]]];
        // Not over a rectangle still being drawn; it is put back when that
        // drag ends.
        if (pointer === null) restore();
        if (!held) continue;
        if (!fitted) {
          map.fitBounds(held, { padding: [20, 20] });
          fitted = true;
        }
      } else if ("armed" in message) {
        arm(message.armed);
      } else if ("marks" in message) {
        heat.clearLayers();
        points = message.marks.points;
        for (const point of points) {
          L.circleMarker(point, {
            radius: 8,
            stroke: false,
            fillColor: "#d7301f",
            fillOpacity: message.marks.opacity,
            interactive: false,
            className: "heat-mark",
          }).addTo(heat);
        }
        if (!fitted && points.length > 0) {
          fitToMarks();
          fitted = true;
        }
      }
    }
"##;

/// What Rust tells the region map, keyed by kind: `{"region": [..]}` (or
/// `null` for none), `{"armed": bool}` and `{"marks": {..}}`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum MapMessage<'a> {
    Region(Option<[f64; 4]>),
    Armed(bool),
    Marks(&'a HeatMarks),
}

/// Start the region map, handing it the rectangle the filters already hold.
///
/// The returned handle must be kept alive — and `recv()`ed in a loop — for
/// as long as the map should report drags: it is the channel.
pub fn start_region_map(restore: Option<[f64; 4]>) -> document::Eval {
    let eval = document::eval(REGION_MAP_SCRIPT);
    if let Err(err) = eval.send(MapMessage::Region(restore)) {
        dioxus::logger::tracing::error!("could not seed the region map: {err}");
    }
    eval
}

/// Draw `region` on the region map in place of whatever rectangle it shows,
/// or take the rectangle off when there is none (US-14).
pub fn show_region(map: &document::Eval, region: Option<[f64; 4]>) {
    if let Err(err) = map.send(MapMessage::Region(region)) {
        dioxus::logger::tracing::error!("could not show the region on the map: {err}");
    }
}

/// Arm the region map for drawing, or disarm it (US-65): while armed, a drag
/// draws a rectangle instead of panning.
pub fn arm_region_map(map: &document::Eval, armed: bool) {
    if let Err(err) = map.send(MapMessage::Armed(armed)) {
        dioxus::logger::tracing::error!("could not arm the region map: {err}");
    }
}

/// Replace the marks on the region map with `marks` (US-63), on the map's
/// own channel.
pub fn draw_heat_marks(map: &document::Eval, marks: &HeatMarks) {
    if let Err(err) = map.send(MapMessage::Marks(marks)) {
        dioxus::logger::tracing::error!("could not draw the trips on the map: {err}");
    }
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
    fn the_map_is_told_which_kind_of_message_it_is_reading() {
        // The script tells a rectangle from the marks by the key alone.
        let region = serde_json::to_value(MapMessage::Region(Some([1.0, 2.0, 3.0, 4.0]))).unwrap();
        assert_eq!(
            region,
            serde_json::json!({ "region": [1.0, 2.0, 3.0, 4.0] })
        );
        let cleared = serde_json::to_value(MapMessage::Region(None)).unwrap();
        assert_eq!(cleared, serde_json::json!({ "region": null }));
        let marks = HeatMarks {
            points: vec![[60.0, 11.0]],
            opacity: 0.6,
        };
        let marks = serde_json::to_value(MapMessage::Marks(&marks)).unwrap();
        assert_eq!(
            marks,
            serde_json::json!({ "marks": { "points": [[60.0, 11.0]], "opacity": 0.6 } })
        );
        // US-65: arming and disarming "Select area" is Rust's to decide.
        let armed = serde_json::to_value(MapMessage::Armed(true)).unwrap();
        assert_eq!(armed, serde_json::json!({ "armed": true }));
    }

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
