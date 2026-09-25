//! The map a share's recipient lands on (US-53): every shared trip's track as
//! a line, fitted to them all. Which lines there are is decided in Rust
//! (`shared::overview_lines`); this only draws them and reports which one
//! was clicked.

use dioxus::prelude::*;
use serde::Serialize;

/// Draws into `#overview-map`, over OSM tiles, one line per trip, and sends
/// the trip's id back when its line is clicked or tapped. Drawn into once per
/// mount; a map left in the registry by an earlier mount belongs to a
/// container that is gone, and is removed — the track map's disposal rule.
const OVERVIEW_MAP_SCRIPT: &str = r##"
    const CONTAINER = "overview-map";
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
    for (const line of view.lines || []) {
      if (line.points.length === 0) continue;
      const drawn = L.polyline(line.points, { color: "#3367d6", weight: 4 }).addTo(map);
      drawn.bindTooltip(line.name, { sticky: true });
      drawn.on("click", () => {
        try {
          dioxus.send(line.id);
        } catch {
          // The screen is gone; nothing is listening any more.
        }
      });
      bounds.extend(drawn.getBounds());
    }
    if (bounds.isValid()) {
      map.fitBounds(bounds, { maxZoom: 15, padding: [16, 16] });
    }
"##;

/// One shared trip's line on the overview map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverviewLine {
    pub id: i64,
    pub name: String,
    /// `[lat, lon]`, the order Leaflet takes.
    pub points: Vec<[f64; 2]>,
}

/// Start the overview map. The handle is the channel: every click on a line
/// arrives on it as that trip's id.
pub fn start_overview_map(lines: Vec<OverviewLine>) -> document::Eval {
    #[derive(Serialize)]
    struct View {
        lines: Vec<OverviewLine>,
    }
    let eval = document::eval(OVERVIEW_MAP_SCRIPT);
    if let Err(err) = eval.send(View { lines }) {
        dioxus::logger::tracing::error!("could not draw the overview map: {err}");
    }
    eval
}
