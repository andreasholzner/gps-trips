//! A guide book's waypoints on a map (US-86): markers in a style no track
//! uses, named on hover or tap. Shared by the maps that draw them — a
//! collection's, and a collection track's — as a function their scripts
//! start with, so the two cannot drift apart.

use serde::Serialize;
use trip_archive_types::Waypoint;

/// Defines `drawWaypoints(map, waypoints)`, which adds the markers to `map`
/// and returns them as a feature group, for the caller to frame and to
/// remove on a redraw.
///
/// A diamond, as a `divIcon`: markup rather than Leaflet's default pin,
/// whose image the bundle does not ship (the photo markers' reason too), and
/// a shape no line, photo circle or hover ring has (`detail.css`).
const WAYPOINTS_JS: &str = r##"
    function drawWaypoints(map, waypoints) {
      return L.featureGroup(
        (waypoints || []).map((waypoint) => {
          const marker = L.marker([waypoint.lat, waypoint.lon], {
            icon: L.divIcon({ className: "waypoint-mark", iconSize: [12, 12] }),
            keyboard: false,
          });
          if (waypoint.name) {
            // A node, not the string: Leaflet puts a string tooltip in as
            // HTML, and the name comes from someone else's file.
            marker.bindTooltip(document.createTextNode(waypoint.name), {
              direction: "top",
              offset: [0, -6],
            });
            // A finger does not hover: a tap names it.
            marker.on("click", () => marker.openTooltip());
          }
          return marker;
        }),
      ).addTo(map);
    }
"##;

/// `script`, able to call `drawWaypoints`.
pub(super) fn with_waypoints(script: &str) -> String {
    format!("{WAYPOINTS_JS}\n{script}")
}

/// One waypoint as a map draws it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapWaypoint {
    pub name: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

impl From<&Waypoint> for MapWaypoint {
    fn from(waypoint: &Waypoint) -> Self {
        Self {
            name: waypoint.name.clone(),
            lat: waypoint.lat,
            lon: waypoint.lon,
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waypoint_reaches_the_map_with_its_name_and_position_only() {
        let waypoint = Waypoint {
            id: 3,
            collection_track_id: Some(1),
            name: Some("Albula Pass".to_string()),
            description: Some("Top".to_string()),
            lat: 46.58,
            lon: 9.84,
            ele: Some(2315.0),
        };

        let sent = serde_json::to_value(MapWaypoint::from(&waypoint)).unwrap();

        assert_eq!(
            sent,
            serde_json::json!({ "name": "Albula Pass", "lat": 46.58, "lon": 9.84 })
        );
    }

    #[test]
    fn a_script_that_draws_waypoints_can_call_the_function_first() {
        let script = with_waypoints("drawWaypoints(map, []);");

        assert!(script.find("function drawWaypoints") < script.find("drawWaypoints(map"));
    }
}
