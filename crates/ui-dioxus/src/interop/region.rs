//! The region map (US-52/US-14, US-63, US-73) — the trip list's own widget.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::heat::HeatMarks;
use crate::trip_lines::{TripLines, Viewport};

/// Coordinate decimals kept in the `bbox` parameter. Six is ~10 cm — far
/// finer than a rectangle dragged by hand needs, and it keeps a shared URL
/// readable.
const BBOX_DECIMALS: usize = 6;

/// The region map (US-52/US-14, US-63, US-65, US-73). Draws into
/// `#region-map`, restores the rectangle it is given, and reports back
/// [`RegionEvent`]s: every finished drag while armed, where the map is
/// looking whenever it settles, and a click on a trip's line.
///
/// Its channel carries [`MapMessage`]s: the rectangle the filters hold —
/// first when the map starts, then whenever the region changes, including
/// to none when it is cleared — and either the marks or the lines whenever
/// the list's rows or the view change; each replaces the other. The view
/// fits once — to the first rectangle, or else to the
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
    const report = (event) => {
      try {
        dioxus.send(event);
      } catch {
        // The screen is gone; nothing is listening any more.
      }
    };
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

    const clampLat = (n) => Math.min(90, Math.max(-90, n));
    const corners = (w) => [
      w.getWest(), clampLat(w.getSouth()),
      w.getEast(), clampLat(w.getNorth()),
    ];

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
      report({ region: corners(w) });
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
    // The trips' own lines once zoomed in (US-73). `trip-line` names them
    // for the browser tests, and for the style that has them take no
    // pointer while armed.
    const lines = L.layerGroup().addTo(map);
    let points = [];
    const fitToMarks = () => {
      if (points.length === 0) return;
      map.fitBounds(L.latLngBounds(points), { padding: [20, 20], maxZoom: 12 });
    };
    document.getElementById("region-fit")?.addEventListener("click", fitToMarks);
    const fitOnce = () => {
      if (!fitted && points.length > 0) {
        fitToMarks();
        fitted = true;
      }
    };

    // Where the map is looking, whenever it settles, for Rust to decide
    // between marks and lines (US-73). Wrapped like a drawn rectangle.
    const reportView = () => {
      report({ view: {
        zoom: map.getZoom(),
        bounds: corners(map.wrapLatLngBounds(map.getBounds())),
      } });
    };
    map.on("moveend", reportView);
    reportView();

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
        lines.clearLayers();
        const marks = message.marks.marks;
        points = marks.map((mark) => mark.at);
        for (const mark of marks) {
          L.circleMarker(mark.at, {
            radius: 8,
            stroke: false,
            fillColor: mark.color,
            fillOpacity: message.marks.opacity,
            interactive: false,
            className: "heat-mark",
          }).addTo(heat);
        }
        fitOnce();
      } else if ("lines" in message) {
        heat.clearLayers();
        lines.clearLayers();
        points = message.lines.fit;
        for (const line of message.lines.lines) {
          if (line.points.length === 0) continue;
          const path = L.polyline(line.points, {
            color: line.color,
            weight: 4,
            className: "trip-line",
          }).addTo(lines);
          // A node, not the string: Leaflet puts a string tooltip in as
          // HTML, and a trip's name comes from a GPX or a Komoot title.
          path.bindTooltip(document.createTextNode(line.name), { sticky: true });
          path.on("click", () => report({ open: line.id }));
        }
        fitOnce();
      }
    }
"##;

/// What Rust tells the region map, keyed by kind: `{"region": [..]}` (or
/// `null` for none), `{"armed": bool}`, `{"marks": {..}}` and
/// `{"lines": {..}}`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum MapMessage<'a> {
    Region(Option<[f64; 4]>),
    Armed(bool),
    Marks(&'a HeatMarks),
    Lines(&'a TripLines),
}

/// What the region map reports, keyed by kind.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionEvent {
    /// A finished drag while armed: `[west, south, east, north]`.
    Region([f64; 4]),
    /// Where the map is looking, once it has settled (US-73).
    View(Viewport),
    /// A trip's line was clicked or tapped: open the trip (US-73).
    Open(i64),
}

/// Start the region map, handing it the rectangle the filters already hold.
///
/// The returned handle must be kept alive — and `recv()`ed in a loop, as
/// [`RegionEvent`]s — for as long as the map should report: it is the
/// channel.
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

/// Replace the marks, or the lines, on the region map with `lines` (US-73).
pub fn draw_trip_lines(map: &document::Eval, lines: &TripLines) {
    if let Err(err) = map.send(MapMessage::Lines(lines)) {
        dioxus::logger::tracing::error!("could not draw the trips' lines on the map: {err}");
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
    use crate::heat::HeatMark;
    use trip_archive_types::ActivityType;

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
            marks: vec![HeatMark {
                at: [60.0, 11.0],
                activity: ActivityType::Hiking,
                color: "#b2182b",
            }],
            opacity: 0.6,
        };
        let marks = serde_json::to_value(MapMessage::Marks(&marks)).unwrap();
        // The activity itself stays in Rust: the script draws the color.
        assert_eq!(
            marks,
            serde_json::json!({ "marks": {
                "marks": [{ "at": [60.0, 11.0], "color": "#b2182b" }],
                "opacity": 0.6,
            } })
        );
        // US-65: arming and disarming "Select area" is Rust's to decide.
        let armed = serde_json::to_value(MapMessage::Armed(true)).unwrap();
        assert_eq!(armed, serde_json::json!({ "armed": true }));
    }

    #[test]
    fn the_map_is_handed_the_lines_to_draw_in_place_of_the_marks() {
        // US-73.
        let lines = TripLines {
            lines: vec![crate::interop::OverviewLine {
                id: 7,
                name: "Ridge".to_string(),
                color: "#b2182b",
                points: vec![[60.0, 11.0]],
            }],
            fit: vec![[60.0, 11.0]],
        };

        assert_eq!(
            serde_json::to_value(MapMessage::Lines(&lines)).unwrap(),
            serde_json::json!({ "lines": {
                "lines": [{ "id": 7, "name": "Ridge", "color": "#b2182b", "points": [[60.0, 11.0]] }],
                "fit": [[60.0, 11.0]],
            } })
        );
    }

    #[test]
    fn the_map_reports_a_rectangle_its_view_and_a_click_apart() {
        let read = |json| serde_json::from_value::<RegionEvent>(json).unwrap();

        assert_eq!(
            read(serde_json::json!({ "region": [1.0, 2.0, 3.0, 4.0] })),
            RegionEvent::Region([1.0, 2.0, 3.0, 4.0])
        );
        assert_eq!(
            read(serde_json::json!({ "view": { "zoom": 11, "bounds": [1.0, 2.0, 3.0, 4.0] } })),
            RegionEvent::View(Viewport {
                zoom: 11.0,
                bounds: [1.0, 2.0, 3.0, 4.0]
            })
        );
        assert_eq!(read(serde_json::json!({ "open": 7 })), RegionEvent::Open(7));
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
