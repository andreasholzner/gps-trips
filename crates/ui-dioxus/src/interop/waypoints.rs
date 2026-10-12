//! A collection's waypoints on a map (US-86): markers in a style no track
//! uses, named on hover or tap — the icon of their symbol where there is one
//! (US-95). Shared by the maps that draw them — a
//! collection's, and a collection track's — as a function their scripts
//! start with, so the two cannot drift apart.

use serde::Serialize;
use trip_archive_types::Waypoint;

use crate::waypoint_icon::{self, WaypointIcon, WAYPOINT_MARK};

/// Defines `drawWaypoints(map, waypoints)`, which adds the markers to `map`
/// and returns them as a feature group, for the caller to frame and to
/// remove on a redraw.
///
/// A diamond, as a `divIcon`: markup rather than Leaflet's default pin,
/// whose image the bundle does not ship (the photo markers' reason too), and
/// a shape no line, photo circle or hover ring has (`detail.css`). A
/// waypoint with an icon gets it instead. Either is an SVG built node by
/// node — an icon's path comes from Rust, but markup would still be HTML put
/// in. The diamond is drawn as one, not turned in CSS: a `rotate` on the
/// marker turns Leaflet's placing transform with it, about the map pane's
/// corner.
const WAYPOINTS_JS: &str = r##"
    function waypointIcon(icon) {
      const ns = "http://www.w3.org/2000/svg";
      const svg = document.createElementNS(ns, "svg");
      svg.setAttribute("viewBox", "0 0 24 24");
      const path = document.createElementNS(ns, "path");
      if (!icon) {
        path.setAttribute("d", WAYPOINT_MARK);
        path.setAttribute("class", "mark");
      } else if (icon.kind === "glyph") {
        const badge = document.createElementNS(ns, "circle");
        badge.setAttribute("class", "badge");
        badge.setAttribute("cx", "12");
        badge.setAttribute("cy", "12");
        badge.setAttribute("r", "12");
        svg.appendChild(badge);
        path.setAttribute("d", icon.path);
        path.setAttribute("class", "glyph");
        path.setAttribute("transform", "translate(12 12) scale(0.62) translate(-12 -12)");
      } else {
        path.setAttribute("d", icon.path);
        path.setAttribute("class", "shape");
        path.setAttribute("fill", icon.color);
      }
      svg.appendChild(path);
      if (icon && icon.half) {
        const half = document.createElementNS(ns, "path");
        half.setAttribute("d", icon.half.path);
        half.setAttribute("class", "half");
        half.setAttribute("fill", icon.half.color);
        svg.appendChild(half);
      }
      return L.divIcon({
        className: icon ? "waypoint-symbol" : "waypoint-mark",
        html: svg,
        iconSize: [16, 16],
      });
    }

    function drawWaypoints(map, waypoints) {
      return L.featureGroup(
        (waypoints || []).map((waypoint) => {
          const marker = L.marker([waypoint.lat, waypoint.lon], {
            icon: waypointIcon(waypoint.icon),
            keyboard: false,
          });
          if (waypoint.name) {
            // A node, not the string: Leaflet puts a string tooltip in as
            // HTML, and the name comes from someone else's file.
            marker.bindTooltip(document.createTextNode(waypoint.name), {
              direction: "top",
              offset: [0, -8],
            });
            // A finger does not hover: a tap names it.
            marker.on("click", () => marker.openTooltip());
          }
          return marker;
        }),
      ).addTo(map);
    }
"##;

/// `script`, able to call `drawWaypoints`. The plain marker's path is the
/// one the pages draw (US-98), handed over rather than written twice.
pub(super) fn with_waypoints(script: &str) -> String {
    format!("const WAYPOINT_MARK = {WAYPOINT_MARK:?};\n{WAYPOINTS_JS}\n{script}")
}

/// One waypoint as a map draws it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapWaypoint {
    pub name: Option<String>,
    pub lat: f64,
    pub lon: f64,
    /// Left out for the plain marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<WaypointIcon>,
}

impl From<&Waypoint> for MapWaypoint {
    fn from(waypoint: &Waypoint) -> Self {
        Self {
            name: waypoint.name.clone(),
            lat: waypoint.lat,
            lon: waypoint.lon,
            icon: waypoint.symbol.as_deref().and_then(waypoint_icon::icon),
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn albula(symbol: Option<&str>) -> Waypoint {
        Waypoint {
            id: 3,
            collection_track_id: Some(1),
            name: Some("Albula Pass".to_string()),
            description: Some("Top".to_string()),
            lat: 46.58,
            lon: 9.84,
            ele: Some(2315.0),
            symbol: symbol.map(str::to_string),
        }
    }

    #[test]
    fn a_waypoint_reaches_the_map_with_its_name_and_position_only() {
        let sent = serde_json::to_value(MapWaypoint::from(&albula(None))).unwrap();

        assert_eq!(
            sent,
            serde_json::json!({ "name": "Albula Pass", "lat": 46.58, "lon": 9.84 })
        );
    }

    #[test]
    fn us95_a_waypoint_with_a_symbol_reaches_the_map_with_its_icon() {
        let sent = serde_json::to_value(MapWaypoint::from(&albula(Some("Circle, Blue")))).unwrap();

        assert_eq!(sent["icon"]["kind"], "shape");
        assert_eq!(sent["icon"]["color"], "#1f7fe0");
        assert!(sent["icon"]["path"]
            .as_str()
            .is_some_and(|path| path.starts_with('M')));
    }

    #[test]
    fn us95_a_symbol_without_an_icon_keeps_the_marker() {
        let sent = serde_json::to_value(MapWaypoint::from(&albula(Some("Shipwreck")))).unwrap();

        assert!(sent.get("icon").is_none(), "{sent}");
    }

    #[test]
    fn us95_the_icon_is_built_as_nodes_never_as_markup() {
        // The SVG goes in as an element; an `innerHTML` or a string `html`
        // would be markup put in.
        assert!(WAYPOINTS_JS.contains("createElementNS"));
        assert!(!WAYPOINTS_JS.contains("innerHTML"));
        assert!(WAYPOINTS_JS.contains("html: svg"));
    }

    #[test]
    fn us95_a_two_tone_icon_draws_its_half_over_the_shape() {
        let sent =
            serde_json::to_value(MapWaypoint::from(&albula(Some("Navaid, Green/Red")))).unwrap();

        assert_eq!(sent["icon"]["half"]["color"], "#e0322b");
        assert!(WAYPOINTS_JS.contains("icon.half"));
    }

    #[test]
    fn us86_the_plain_marker_is_a_drawn_diamond_never_a_turned_box() {
        // A CSS `rotate` on the marker turns Leaflet's placing transform
        // too, and the marker lands away from its place.
        assert!(WAYPOINTS_JS.contains(r#"path.setAttribute("d", WAYPOINT_MARK)"#));
        assert!(!include_str!("../../assets/detail.css").contains("rotate:"));
    }

    #[test]
    fn us98_the_map_draws_the_same_plain_marker_as_the_pages() {
        let script = with_waypoints("");

        assert!(script.contains(&format!(r#"const WAYPOINT_MARK = "{WAYPOINT_MARK}";"#)));
    }

    #[test]
    fn a_script_that_draws_waypoints_can_call_the_function_first() {
        let script = with_waypoints("drawWaypoints(map, []);");

        assert!(script.find("function drawWaypoints") < script.find("drawWaypoints(map"));
    }
}
