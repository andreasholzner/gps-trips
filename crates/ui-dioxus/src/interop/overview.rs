//! The map a share's recipient lands on (US-53): every shared trip's track as
//! a line, fitted to them all. Which lines there are is decided in Rust
//! (`shared::overview_lines`); this only draws them and reports which one
//! was clicked.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// Draws into `#overview-map`, over OSM tiles, one line per trip in the
/// color it is given, and sends
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

    const WEIGHT = 4;
    const HIGHLIGHTED = 7;
    const report = (event) => {
      try {
        dioxus.send(event);
      } catch {
        // The screen is gone; nothing is listening any more.
      }
    };

    const view = await dioxus.recv();
    const bounds = L.latLngBounds([]);
    const drawn = new Map();
    for (const line of view.lines || []) {
      if (line.points.length === 0) continue;
      const path = L.polyline(line.points, { color: line.color, weight: WEIGHT }).addTo(map);
      // A node, not the string: Leaflet puts a string tooltip in as HTML, and
      // a trip's name comes from a GPX `<name>` or a Komoot title.
      path.bindTooltip(document.createTextNode(line.name), { sticky: true });
      path.on("click", () => report({ open: line.id }));
      path.on("mouseover", () => report({ hover: line.id }));
      path.on("mouseout", () => report({ hover: null }));
      drawn.set(line.id, path);
      bounds.extend(path.getBounds());
    }
    if (bounds.isValid()) {
      map.fitBounds(bounds, { maxZoom: 15, padding: [16, 16] });
    }

    // Then which line to highlight (US-72), for as long as the screen is
    // there. Rust decides — a row's hover and a line's hover both end up
    // here — so this end only restyles.
    let highlighted = null;
    for (;;) {
      let message;
      try {
        message = await dioxus.recv();
      } catch {
        return;
      }
      if (highlighted) highlighted.setStyle({ weight: WEIGHT });
      highlighted = drawn.get(message.highlight) || null;
      if (highlighted) {
        highlighted.setStyle({ weight: HIGHLIGHTED });
        highlighted.bringToFront();
      }
    }
"##;

/// One shared trip's line on the overview map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverviewLine {
    pub id: i64,
    pub name: String,
    /// The trip's activity color (US-75).
    pub color: &'static str,
    /// `[lat, lon]`, the order Leaflet takes.
    pub points: Vec<[f64; 2]>,
}

/// What the map reports about a trip's line.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewEvent {
    /// Clicked or tapped: open the trip.
    Open(i64),
    /// The pointer went onto the line, or off it (US-72).
    Hover(Option<i64>),
}

/// What Rust tells the map once it is drawn.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum OverviewMessage {
    /// The trip whose line stands out, or none (US-72).
    Highlight(Option<i64>),
}

/// Start the overview map. The handle is the channel, read for the map's
/// whole life: every click on a line and every hover arrives on it as an
/// [`OverviewEvent`].
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

/// Make `id`'s line stand out on the overview map, or none (US-72).
pub fn highlight_on_overview_map(map: &document::Eval, id: Option<i64>) {
    if let Err(err) = map.send(OverviewMessage::Highlight(id)) {
        dioxus::logger::tracing::error!("could not highlight a line: {err}");
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_reports_a_click_and_a_hover_apart() {
        // US-72: a click opens the trip; a hover only points at it.
        let open: OverviewEvent = serde_json::from_value(serde_json::json!({ "open": 7 })).unwrap();
        let over: OverviewEvent =
            serde_json::from_value(serde_json::json!({ "hover": 7 })).unwrap();
        let off: OverviewEvent =
            serde_json::from_value(serde_json::json!({ "hover": null })).unwrap();

        assert_eq!(open, OverviewEvent::Open(7));
        assert_eq!(over, OverviewEvent::Hover(Some(7)));
        assert_eq!(off, OverviewEvent::Hover(None));
    }

    #[test]
    fn the_map_is_told_which_line_to_highlight() {
        let on = serde_json::to_value(OverviewMessage::Highlight(Some(7))).unwrap();
        let off = serde_json::to_value(OverviewMessage::Highlight(None)).unwrap();

        assert_eq!(on, serde_json::json!({ "highlight": 7 }));
        assert_eq!(off, serde_json::json!({ "highlight": null }));
    }
}
