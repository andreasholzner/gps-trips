//! The map a share's recipient lands on (US-53), and a collection's
//! collection page (US-86): every track as a line, fitted to them all. Which lines there are is decided in Rust
//! (`shared::overview_lines`); this only draws them and reports which one
//! was clicked.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::overlays::{with_overlays, Overlays};
use super::region::Opened;
use super::waypoints::MapWaypoint;
use crate::activity_icon::MapIcon;
use crate::map_items::MapSource;
use crate::Route;

/// Draws into `#overview-map`, over OSM tiles, one line per trip in the
/// color it is given, and sends back what is chosen of what lies under the
/// pointer (US-104): a trip's id, a waypoint's, or a linked line's place. Drawn into once per
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
    // The view of an earlier draw into this same container, with what it
    // framed: kept when only the ticked overlays changed (US-105).
    let earlier = null;
    if (widgets[CONTAINER]) {
      const old = widgets[CONTAINER];
      if (old.getContainer() === el && old.framed) {
        earlier = { framed: old.framed, center: old.getCenter(), zoom: old.getZoom() };
      }
      old.remove();
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
    // What the pointer can be over (US-104), in the page's order.
    const picks = [];
    for (const line of view.lines || []) {
      if (line.points.length === 0) continue;
      // In its kind's line style (US-107), as the region map draws it.
      const path = L.polyline(line.points, {
        color: line.color,
        weight: WEIGHT,
        dashArray: line.dash || null,
      }).addTo(map);
      // A line with a source is opened by it: a trip and a collection track
      // may share an id (US-107).
      const opens = line.source ? { open_item: { source: line.source, id: line.id } } : { open: line.id };
      // Pointed at, it says so; Rust decides what stands out (US-72).
      picks.push(pickLine(path, line, opens, (on) => report({ hover: on ? line.id : null })));
      drawn.set(line.id, path);
      bounds.extend(path.getBounds());
    }
    // A collection's waypoints (US-86); none on a share's or a summary's.
    const waypoints = drawWaypoints(map, view.waypoints);
    if (waypoints.getLayers().length > 0) bounds.extend(waypoints.getBounds());
    // What the page is linked to that the owner ticked (US-99) — left out of
    // the framing, so ticking one leaves the view where it is (US-105).
    const overlays = drawOverlays(map, view.overlays);
    setPicking(map, [...picks, ...waypointPicks(map, waypoints), ...overlays.picks], report);
    if (bounds.isValid()) {
      map.framed = bounds.toBBoxString();
      if (earlier && earlier.framed === map.framed) {
        map.setView(earlier.center, earlier.zoom, { animate: false });
      } else {
        map.fitBounds(bounds, { maxZoom: 15, padding: [16, 16] });
      }
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
    /// What leads its name in the map's hint and menu (US-104).
    pub icon: Option<MapIcon>,
    /// Its kind's Leaflet `dashArray` (US-107); `None` for a solid line.
    pub dash: Option<&'static str>,
    /// Where it comes from, on a map holding trips and collection tracks
    /// alike (US-107): choosing it then reports [`OverviewEvent::OpenItem`].
    /// `None` reports [`OverviewEvent::Open`] with its id alone.
    pub source: Option<MapSource>,
    /// `[lat, lon]`, the order Leaflet takes.
    pub points: Vec<[f64; 2]>,
    /// The address of the page choosing it opens, for a Ctrl- or ⌘-click to
    /// open in a new tab (US-114); see [`OverviewLine::opening`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
}

impl OverviewLine {
    /// What the map reports when it is chosen: by its source if it has one
    /// (US-107), else by its id alone — as the script does.
    fn chosen(&self) -> OverviewEvent {
        match self.source {
            Some(source) => OverviewEvent::OpenItem(Opened {
                source,
                id: self.id,
            }),
            None => OverviewEvent::Open(self.id),
        }
    }
}

/// `lines`, each with the address of the page `page_of` opens when it is
/// chosen, for a Ctrl- or ⌘-click to open in a new tab (US-114). The same
/// `page_of` a screen opens a chosen line with, so both open one page.
pub fn opening_pages(
    lines: Vec<OverviewLine>,
    page_of: impl Fn(OverviewEvent) -> Option<Route>,
) -> Vec<OverviewLine> {
    lines
        .into_iter()
        .map(|line| OverviewLine {
            href: page_of(line.chosen()).map(|to| crate::page_href(&to)),
            ..line
        })
        .collect()
}

/// What the map reports about a trip's line.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewEvent {
    /// Clicked or tapped: open the trip.
    Open(i64),
    /// The pointer went onto the line, or off it (US-72).
    Hover(Option<i64>),
    /// A waypoint was chosen: open its page (US-104).
    OpenWaypoint(i64),
    /// The `n`th line a ticked link put there was chosen (US-104).
    OpenOverlay(usize),
    /// A line with a source was chosen: open its trip or collection track
    /// (US-107).
    OpenItem(Opened),
}

/// What Rust tells the map once it is drawn.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum OverviewMessage {
    /// The trip whose line stands out, or none (US-72).
    Highlight(Option<i64>),
}

/// Start the overview map, with a collection's `waypoints` among its lines
/// (US-86), and the `overlays` a page's links put there (US-99). The handle is the channel, read for the map's whole life: every
/// click on a line and every hover arrives on it as an [`OverviewEvent`].
pub fn start_overview_map(
    lines: Vec<OverviewLine>,
    waypoints: Vec<MapWaypoint>,
    overlays: Overlays,
) -> document::Eval {
    #[derive(Serialize)]
    struct View {
        lines: Vec<OverviewLine>,
        waypoints: Vec<MapWaypoint>,
        overlays: Overlays,
    }
    let eval = document::eval(&with_overlays(OVERVIEW_MAP_SCRIPT));
    if let Err(err) = eval.send(View {
        lines,
        waypoints,
        overlays,
    }) {
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
    use crate::kinds::Kind;

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
    fn us104_the_map_reports_a_waypoint_or_a_linked_line_chosen() {
        let waypoint: OverviewEvent =
            serde_json::from_value(serde_json::json!({ "open_waypoint": 4 })).unwrap();
        let overlay: OverviewEvent =
            serde_json::from_value(serde_json::json!({ "open_overlay": 1 })).unwrap();

        assert_eq!(waypoint, OverviewEvent::OpenWaypoint(4));
        assert_eq!(overlay, OverviewEvent::OpenOverlay(1));
    }

    #[test]
    fn us107_a_line_reaches_the_map_in_its_kinds_style_and_with_its_source() {
        let line = OverviewLine {
            id: 7,
            name: "Stage 1".to_string(),
            color: "#1f4e9c",
            icon: None,
            dash: Kind::Collections.dash(),
            source: Some(MapSource::CollectionTracks),
            points: vec![[46.5, 9.1]],
            href: None,
        };

        let sent = serde_json::to_value(&line).unwrap();

        assert_eq!(sent["dash"], "1 7");
        assert_eq!(sent["source"], "collection_tracks");
    }

    #[test]
    fn us114_a_line_reaches_the_map_with_the_address_of_the_page_a_click_opens() {
        let line = OverviewLine {
            id: 7,
            name: "Stage 1".to_string(),
            color: "#1f4e9c",
            icon: None,
            dash: None,
            source: None,
            points: vec![[46.5, 9.1]],
            href: None,
        };

        let plain = serde_json::to_value(&line).unwrap();
        let sourced = OverviewLine {
            id: 8,
            source: Some(MapSource::Trips),
            ..line.clone()
        };
        let opening = opening_pages(vec![line, sourced], |event| match event {
            OverviewEvent::Open(id) => Some(Route::CollectionTrack { id }),
            OverviewEvent::OpenItem(opened) => Some(Route::TripDetail { id: opened.id }),
            _ => None,
        });
        let nothing = opening_pages(opening.clone(), |_| None);

        assert!(plain.get("href").is_none(), "{plain}");
        let sent = serde_json::to_value(&opening).unwrap();
        assert_eq!(sent[0]["href"], "/collection-tracks/7");
        assert_eq!(sent[1]["href"], "/trips/8", "chosen by its source");
        assert!(nothing.iter().all(|line| line.href.is_none()));
    }

    #[test]
    fn us107_a_line_with_a_source_is_reported_by_it_when_chosen() {
        let opened: OverviewEvent = serde_json::from_value(serde_json::json!({
            "open_item": { "source": "collection_tracks", "id": 7 }
        }))
        .unwrap();

        assert_eq!(
            opened,
            OverviewEvent::OpenItem(Opened {
                source: MapSource::CollectionTracks,
                id: 7,
            })
        );
    }

    #[test]
    fn the_map_is_told_which_line_to_highlight() {
        let on = serde_json::to_value(OverviewMessage::Highlight(Some(7))).unwrap();
        let off = serde_json::to_value(OverviewMessage::Highlight(None)).unwrap();

        assert_eq!(on, serde_json::json!({ "highlight": 7 }));
        assert_eq!(off, serde_json::json!({ "highlight": null }));
    }
}
