//! What a page's map shows of what the page is linked to (US-99), once the
//! owner ticks it: each linked trip or track as a line in its activity's
//! color and its kind's line style (US-90), each linked waypoint as its
//! icon. Drawn by the track map and the overview map alike, over what they
//! already draw.

use serde::Serialize;

use super::picking::with_picking;
use super::waypoints::MapWaypoint;
use crate::activity_icon::MapIcon;
use crate::Route;

/// Draws `overlays` into `map` as one layer, which the caller removes before
/// drawing anew, and returns it with what of it can be picked (US-104), in
/// the links' order: each line reports its place among the lines, for Rust
/// to open the page it leads to, and each waypoint its own page.
const OVERLAYS_JS: &str = r##"
    function drawOverlays(map, overlays) {
      const group = L.featureGroup().addTo(map);
      group.picks = [];
      ((overlays && overlays.lines) || []).forEach((line, index) => {
        if (line.points.length === 0) return;
        const path = L.polyline(line.points, {
          color: line.color,
          weight: 3,
          dashArray: line.dash || null,
          lineCap: "round",
        });
        group.addLayer(path);
        group.picks.push(pickLine(path, line, { open_overlay: index }));
      });
      // Taken off the map in their own group first, so moving them into this
      // one is what puts them back.
      const waypoints = drawWaypoints(map, (overlays && overlays.waypoints) || []);
      map.removeLayer(waypoints);
      for (const marker of waypoints.getLayers()) group.addLayer(marker);
      group.picks.push(...waypointPicks(map, waypoints));
      return group;
    }
"##;

/// `script` with what it needs to draw overlays — and to pick from them, and
/// waypoints, which overlays draw with.
pub(super) fn with_overlays(script: &str) -> String {
    with_picking(&format!("{OVERLAYS_JS}\n{script}"))
}

/// One linked trip or track on the map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverlayLine {
    pub name: String,
    /// Its activity's color (US-75).
    pub color: &'static str,
    /// What leads its name in the map's hint and menu (US-104).
    pub icon: Option<MapIcon>,
    /// The page choosing it opens (US-104), kept here: the map reports
    /// which line was chosen, never where it leads. It is given the page's
    /// address alone, for a Ctrl- or ⌘-click to open in a new tab (US-114).
    #[serde(rename = "href", serialize_with = "as_href")]
    pub to: Route,
    /// Its kind's Leaflet `dashArray` (US-90); `None` for a solid line.
    pub dash: Option<&'static str>,
    /// `[lat, lon]`, the order Leaflet takes.
    pub points: Vec<[f64; 2]>,
}

fn as_href<S: serde::Serializer>(to: &Route, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&crate::page_href(to))
}

/// Everything ticked to be shown, in the order it is listed.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Overlays {
    pub lines: Vec<OverlayLine>,
    pub waypoints: Vec<MapWaypoint>,
}

impl Overlays {
    /// The page the `n`th line opens (US-104).
    pub fn route(&self, n: usize) -> Option<Route> {
        self.lines.get(n).map(|line| line.to.clone())
    }
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
                icon: None,
                to: crate::Route::TripDetail { id: 2 },
                dash: Some("8 6"),
                points: vec![[46.5, 9.1]],
            }],
            waypoints: Vec::new(),
        };

        assert_eq!(
            serde_json::to_value(&overlays).unwrap(),
            serde_json::json!({
                // Which page it leads to is Rust's (US-104); the map is given
                // its address for a Ctrl- or ⌘-click alone (US-114).
                "lines": [{
                    "name": "Plan", "color": "#1f4e9c", "icon": null, "href": "/trips/2",
                    "dash": "8 6", "points": [[46.5, 9.1]],
                }],
                "waypoints": [],
            })
        );
    }

    #[test]
    fn us99_the_drawing_code_comes_with_what_it_draws_waypoints_with() {
        let script = with_overlays("drawOverlays(map, view.overlays);");

        assert!(script.contains("function drawOverlays"), "{script}");
        assert!(script.contains("function drawWaypoints"), "{script}");
        // US-104: and with what it picks them with.
        assert!(script.contains("function pickLine"), "{script}");
    }
}
