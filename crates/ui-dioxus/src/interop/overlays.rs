//! What a page's map shows of what the page is linked to (US-99), once the
//! owner ticks it: each linked trip or track as a line in its activity's
//! color and its kind's line style (US-90), each linked waypoint as its
//! icon. Drawn by the track map and the overview map alike, over what they
//! already draw.

use serde::Serialize;

use super::waypoints::{with_waypoints, MapWaypoint};

/// Draws `overlays` into `map` as one layer, which the caller removes before
/// drawing anew, and returns it. The lines take no clicks: the page's own
/// list leads to them.
const OVERLAYS_JS: &str = r##"
    function drawOverlays(map, overlays) {
      const lines = ((overlays && overlays.lines) || [])
        .filter((line) => line.points.length > 0)
        .map((line) => {
          const path = L.polyline(line.points, {
            color: line.color,
            weight: 3,
            dashArray: line.dash || null,
            lineCap: "round",
          });
          // A node, not the string: Leaflet puts a string tooltip in as
          // HTML, and a name comes from someone's GPX.
          path.bindTooltip(document.createTextNode(line.name), { sticky: true });
          return path;
        });
      const group = L.featureGroup(lines).addTo(map);
      // Taken off the map in their own group first, so moving them into this
      // one is what puts them back.
      const waypoints = drawWaypoints(map, (overlays && overlays.waypoints) || []);
      map.removeLayer(waypoints);
      for (const marker of waypoints.getLayers()) group.addLayer(marker);
      return group;
    }
"##;

/// `script` with what it needs to draw overlays — and waypoints, which
/// overlays draw with.
pub(super) fn with_overlays(script: &str) -> String {
    with_waypoints(&format!("{OVERLAYS_JS}\n{script}"))
}

/// One linked trip or track on the map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverlayLine {
    pub name: String,
    /// Its activity's color (US-75).
    pub color: &'static str,
    /// Its kind's Leaflet `dashArray` (US-90); `None` for a solid line.
    pub dash: Option<&'static str>,
    /// `[lat, lon]`, the order Leaflet takes.
    pub points: Vec<[f64; 2]>,
}

/// Everything ticked to be shown, in the order it is listed.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Overlays {
    pub lines: Vec<OverlayLine>,
    pub waypoints: Vec<MapWaypoint>,
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us99_a_linked_line_reaches_the_map_with_its_style() {
        let overlays = Overlays {
            lines: vec![OverlayLine {
                name: "Plan".to_string(),
                color: "#1f4e9c",
                dash: Some("8 6"),
                points: vec![[46.5, 9.1]],
            }],
            waypoints: Vec::new(),
        };

        assert_eq!(
            serde_json::to_value(&overlays).unwrap(),
            serde_json::json!({
                "lines": [{ "name": "Plan", "color": "#1f4e9c", "dash": "8 6", "points": [[46.5, 9.1]] }],
                "waypoints": [],
            })
        );
    }

    #[test]
    fn us99_the_drawing_code_comes_with_what_it_draws_waypoints_with() {
        let script = with_overlays("drawOverlays(map, view.overlays);");

        assert!(script.contains("function drawOverlays"), "{script}");
        assert!(script.contains("function drawWaypoints"), "{script}");
    }
}
