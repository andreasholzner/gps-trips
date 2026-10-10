//! The region map (US-52/US-14, US-63, US-73, US-92) — the trip list's own
//! widget.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::heat::HeatMarks;
use crate::trip_lines::{TripLines, Viewport};

/// Coordinate decimals kept in the `bbox` parameter. Six is ~10 cm — far
/// finer than a region taken from the map's view needs, and it keeps a
/// shared URL readable.
const BBOX_DECIMALS: usize = 6;

/// The region map (US-52/US-14, US-63, US-73, US-92). Draws into
/// `#region-map`, starts on the region it is given, and reports back
/// [`RegionEvent`]s: where the map is looking whenever it settles — after a
/// pan and a zoom alike — and a click on a trip's line.
///
/// Its channel carries [`MapMessage`]s: the region the filters hold, once,
/// when the map starts, and either the marks or the lines whenever the
/// list's rows or the view change; each replaces the other. The view fits
/// once — to that region, or else to the first marks there are — and after
/// that only when the owner asks with "Fit to trips", so the map does not
/// jump on every keystroke.
///
/// Coordinate hygiene stays on the JS side because it needs the live map:
/// Leaflet's world repeats horizontally, so a map panned east reports
/// longitudes like 309 rather than -51, and a view of the blank space above
/// the projected world reaches latitudes beyond ±90 — either of which the
/// server rejects with a 400. `wrapLatLngBounds` shifts the view back by
/// whole world widths, keeping its size and its place on the globe, and the
/// latitudes are clamped. A view that genuinely straddles the antimeridian
/// still ends east of 180; Rust widens that one to every longitude.
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

    map.fitBounds(EUROPE);

    const clampLat = (n) => Math.min(90, Math.max(-90, n));
    const corners = (w) => [
      w.getWest(), clampLat(w.getSouth()),
      w.getEast(), clampLat(w.getNorth()),
    ];

    // Only now read the channel — after the map is interactive, never
    // before. Awaiting it first would leave the map drawn but dead if the
    // first message were slow or never came, which is exactly what it did.
    let fitted = false;

    // The heat marks (US-63): not interactive, so a drag that starts on one
    // still pans. `heat-mark` names them for the browser tests.
    const heat = L.layerGroup().addTo(map);
    // The trips' own lines once zoomed in (US-73). `trip-line` names them
    // for the browser tests.
    const lines = L.layerGroup().addTo(map);
    // Thinner than a share's lines: a dense region draws many at once.
    const LINE_WEIGHT = 3;
    const HIGHLIGHTED_WEIGHT = 6;
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

    // Where the map is looking, whenever it settles — a zoom ends in a
    // `moveend` too — for Rust to decide between marks and lines (US-73)
    // and, while filtering to the map is on, to take as the region (US-92).
    // The first report waits for the region the map starts on: reporting
    // Europe before it would briefly make Europe the region.
    const reportView = () => {
      report({ view: {
        zoom: map.getZoom(),
        bounds: corners(map.wrapLatLngBounds(map.getBounds())),
      } });
    };
    map.on("moveend", reportView);

    for (;;) {
      const message = await dioxus.recv();
      if ("region" in message) {
        // The region the map starts on, from the URL (US-92). Fitted
        // without padding or animation, so the view — and with filtering
        // on, the region — is as close to it as the map's shape allows.
        const region = message.region;
        if (region && !fitted) {
          map.fitBounds([[region[1], region[0]], [region[3], region[2]]], { animate: false });
          fitted = true;
        }
        reportView();
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
            weight: LINE_WEIGHT,
            className: "trip-line",
          }).addTo(lines);
          // A node, not the string: Leaflet puts a string tooltip in as
          // HTML, and a trip's name comes from a GPX or a Komoot title.
          path.bindTooltip(document.createTextNode(line.name), { sticky: true });
          path.on("click", () => report({ open: line.id }));
          // The line under the pointer stands out above the others.
          path.on("mouseover", () => {
            path.setStyle({ weight: HIGHLIGHTED_WEIGHT });
            path.bringToFront();
          });
          path.on("mouseout", () => path.setStyle({ weight: LINE_WEIGHT }));
        }
        fitOnce();
      }
    }
"##;

/// What Rust tells the region map, keyed by kind: `{"region": [..]}` (or
/// `null` for none) to start on, `{"marks": {..}}` and `{"lines": {..}}`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum MapMessage<'a> {
    Region(Option<[f64; 4]>),
    Marks(&'a HeatMarks),
    Lines(&'a TripLines),
}

/// What the region map reports, keyed by kind.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionEvent {
    /// Where the map is looking, once it has settled (US-73).
    View(Viewport),
    /// A trip's line was clicked or tapped: open the trip (US-73).
    Open(i64),
}

/// Start the region map on the region the filters already hold (US-92).
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

/// A region's four corners, as the `bbox` query parameter the API
/// takes (`minLon,minLat,maxLon,maxLat`, ADR-0008).
pub fn bbox_param(corners: [f64; 4]) -> String {
    corners
        .iter()
        .map(|n| format!("{n:.BBOX_DECIMALS$}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// The inverse, for starting the map on a stored region. `None` for
/// anything that is not four numbers — a hand-edited URL loses the region's
/// view rather than breaking the screen.
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
    fn the_map_reports_its_view_and_a_click_apart() {
        let read = |json| serde_json::from_value::<RegionEvent>(json).unwrap();

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
    fn us92_the_map_reports_no_rectangle_any_more() {
        // The visible area is the region; nothing is drawn to report.
        assert!(serde_json::from_value::<RegionEvent>(
            serde_json::json!({ "region": [1.0, 2.0, 3.0, 4.0] })
        )
        .is_err());
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
