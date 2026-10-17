//! The detail screen's track map (US-7), beside its elevation profile
//! (`super::elevation`): handed values Rust has already prepared
//! (`crate::track`) — this script only draws.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::overlays::{with_overlays, Overlays};
use super::waypoints::MapWaypoint;
use crate::activity_icon::MapIcon;
use crate::photos::{PhotoMarker, PopupTap};

/// The track map. Draws into `#track-map`: the polyline it is given, fitted
/// to the view, over OSM raster tiles.
///
/// The tile layer goes up before the polyline arrives, so a payload that is
/// slow or never comes leaves a usable map rather than a blank box — the
/// ordering the region map's spike arrived at (`docs/eval-two-way-spike.md`).
/// A view is set for the same reason: Leaflet refuses to render tiles until
/// it has one, and only the track can say where it should be.
///
/// Unlike the region map, this one is **payload-driven and redrawn**: the
/// same screen shows a different trip when the router swaps `id`. So the
/// instance is kept in a registry rather than the container being flagged as
/// done — a flag would make the second draw a no-op and leave the previous
/// trip's line on the map. The registry doubles as the guard ADR-0025 asks
/// for (a second script reuses the map instead of building another into the
/// same node) and as the disposal point: a container that was unmounted and
/// rebuilt leaves its map orphaned, holding Leaflet's window listeners, and
/// that one is removed rather than left to accumulate.
const TRACK_MAP_SCRIPT: &str = r##"
    const CONTAINER = "track-map";
    // The whole world, until the track says where it actually is.
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

    let map = widgets[CONTAINER];
    if (map && map.getContainer() !== el) {
      map.remove();
      map = null;
    }
    if (!map) {
      map = L.map(el);
      L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
        maxZoom: 19,
        attribution: "© OpenStreetMap contributors",
      }).addTo(map);
      map.fitBounds(EVERYWHERE);
      widgets[CONTAINER] = map;
    }

    // One payload carries the whole picture — the line and the photos on it —
    // so a redraw replaces what is there rather than layering on top of it.
    const view = await dioxus.recv();
    for (const layer of [map.trackLine, map.photoMarkers, map.waypointMarkers, map.overlays, map.hoverMark]) {
      if (layer) map.removeLayer(layer);
    }
    map.trackLine = null;
    map.photoMarkers = null;
    // A collection's waypoints on the track (US-86); none on a trip's.
    map.waypointMarkers = drawWaypoints(map, view.waypoints);
    map.overlays = drawOverlays(map, view.overlays);
    // A mark for a chart that is being replaced points at a track that is
    // no longer drawn.
    map.hoverMark = null;

    // A feature group, not a plain layer group: the view has to be able to
    // include a photo whose EXIF GPS puts it off the track (US-3), which
    // needs the markers' own bounds.
    map.photoMarkers = L.featureGroup(
      (view.markers || []).map((group, groupIndex) => {
        const photos = group.photos || [];
        // Every photo the marker stands for, not just the topmost one
        // (US-57). The popup scrolls once there are more than fit.
        const popup = document.createElement("div");
        popup.className = "photo-popup";
        if (photos.length > 1) {
          const count = document.createElement("p");
          count.className = "photo-popup-count";
          count.textContent = `${photos.length} photos here`;
          popup.appendChild(count);
        }
        // Each photo opens the viewer on this marker's photos (US-62). The
        // viewer is Rust's, so the tap crosses back over this channel — only
        // where in which group, since Rust holds the set it sent.
        photos.forEach((photo, photoIndex) => {
          const open = document.createElement("button");
          open.type = "button";
          open.className = "popup-photo";
          open.title = `Open ${photo.name}`;
          const img = document.createElement("img");
          img.src = photo.thumbnail_url;
          img.alt = photo.name;
          open.appendChild(img);
          open.addEventListener("click", () => {
            try {
              dioxus.send({ photo: { marker: groupIndex, photo: photoIndex } });
            } catch {
              // A superseded draw's channel is closed; its popups are gone
              // with the redraw.
            }
          });
          popup.appendChild(open);
        });

        // Neither shape is Leaflet's default pin: that pin is an image file,
        // and neither the bundle nor the APK ships Leaflet's `images/`
        // directory — every one of them would 404 and leave the photo
        // invisible on the map. A circle for a single photo, and for a group
        // a `divIcon` carrying the count, which is markup rather than an
        // asset for the same reason.
        const marker =
          photos.length > 1
            ? L.marker([group.lat, group.lon], {
                icon: L.divIcon({
                  className: "photo-cluster",
                  html: String(photos.length),
                  // Big enough for the count and no bigger: a single photo's
                  // circle is 14px across, and a badge that dwarfs it reads
                  // as a different kind of thing rather than the same marker
                  // standing for more.
                  iconSize: [20, 20],
                }),
              })
            : L.circleMarker([group.lat, group.lon], {
                radius: 7,
                color: "#ffffff",
                weight: 2,
                fillColor: "#f5a623",
                fillOpacity: 1,
                // A photo opens its popup, never the map's menu (US-104).
                bubblingMouseEvents: false,
              });
        // Nor does the hint name what lies under it.
        marker.on("mouseover", () => map.picking?.hideHint());
        return marker.bindPopup(popup);
      }),
    ).addTo(map);

    const points = view.points || [];
    if (points.length > 0) {
      map.trackLine = L.polyline(points, {
        color: view.color,
        weight: 3,
        dashArray: view.dash || null,
      }).addTo(map);
    }

    // What the pointer can be over (US-104): the page's own track, named
    // but opening nothing, its waypoints, and what its links put there.
    const report = (event) => {
      try {
        dioxus.send(event);
      } catch {
        // A superseded draw's channel is closed; the redraw picks anew.
      }
    };
    setPicking(
      map,
      [
        ...(map.trackLine ? [pickLine(map.trackLine, view, null)] : []),
        ...waypointPicks(map, map.waypointMarkers),
        ...map.overlays.picks,
      ],
      report,
    );

    // Framed on everything there is to see. A track with no drawable
    // positions still has photos, and they should not be left somewhere on a
    // view of the whole world. Not on what the owner ticked to be shown
    // (US-99): ticking one leaves the view where it is (US-105).
    const bounds = L.latLngBounds([]);
    if (map.trackLine) bounds.extend(map.trackLine.getBounds());
    for (const markers of [map.photoMarkers, map.waypointMarkers]) {
      if (markers.getLayers().length > 0) bounds.extend(markers.getBounds());
    }
    if (bounds.isValid()) {
      // Only when there is something new to frame. This script runs again
      // whenever the photos change, and refitting then would throw away a
      // zoom the owner had just made into part of the track.
      const framed = bounds.toBBoxString();
      if (map.framed !== framed) {
        map.fitBounds(bounds);
        map.framed = framed;
      }
    }

    // A pane of the ring's own, above the markers. Every vector here — the
    // track, the photo circles, the ring — belongs to the overlay pane
    // (z-index 400), while a group's badge is a real marker in the marker
    // pane (600), so a ring drawn with the other vectors goes *under* the
    // badge it is meant to point at. 610 keeps it under tooltips and popups,
    // which should still open over it.
    const HOVER_PANE = "hover-mark";
    if (!map.getPane(HOVER_PANE)) {
      map.createPane(HOVER_PANE).style.zIndex = 610;
    }

    // Then stay on the channel for the point the elevation profile is being
    // hovered at (US-59): a position to mark, or null when the cursor has
    // left the chart. Rust resolves the chart's index to a position — the
    // two indices are not the same point — so this end only draws.
    for (;;) {
      let at;
      try {
        at = await dioxus.recv();
      } catch {
        // The channel closed: this draw was superseded, or the screen went
        // away. Whichever it is, the instance that owns the map now is not
        // this one.
        return;
      }
      if (map.hoverMark) {
        map.removeLayer(map.hoverMark);
        map.hoverMark = null;
      }
      if (at) {
        // A ring rather than a dot: the track runs through the point being
        // marked, and an unfilled circle keeps the line visible under it.
        map.hoverMark = L.circleMarker(at, {
          radius: 8,
          color: view.color,
          weight: 3,
          fill: false,
          className: "hover-mark",
          pane: HOVER_PANE,
          // It says where the cursor is; it is not something to click. Left
          // interactive it would swallow clicks meant for a photo marker
          // underneath it, which is the price of drawing it on top.
          interactive: false,
        }).addTo(map);
      }
    }
"##;

/// What the map shows: the track as `[lat, lon]` pairs in its activity's
/// color (US-75), a marker per group of photos taken at the same place
/// (US-3/US-4, grouped by US-57), and a collection's waypoints on the track
/// (US-86). Sent as one value because it is one picture — a redraw with new
/// markers must not lose the line.
#[derive(Serialize)]
struct TrackMapView {
    #[serde(flatten)]
    line: OwnTrack,
    markers: Vec<PhotoMarker>,
    waypoints: Vec<MapWaypoint>,
    /// What the page is linked to that the owner ticked (US-99).
    overlays: Overlays,
}

/// The page's own track: named in the map's hint with its activity's icon,
/// though choosing it opens nothing (US-104).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OwnTrack {
    pub name: String,
    /// Its activity's color (US-75).
    pub color: &'static str,
    pub icon: Option<MapIcon>,
    /// Its kind's Leaflet `dashArray` (US-108); `None` for a solid line.
    pub dash: Option<&'static str>,
    /// `[lat, lon]`, the order Leaflet takes.
    pub points: Vec<[f64; 2]>,
}

/// What the track map reports.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackMapEvent {
    /// A photo tapped in a marker's popup (US-62).
    Photo(PopupTap),
    /// A waypoint was chosen: open its page (US-104).
    OpenWaypoint(i64),
    /// The `n`th line a ticked link put there was chosen (US-104).
    OpenOverlay(usize),
}

/// Start the track map with the line, its color, and the photo markers to
/// draw. The point hovered on the elevation chart is marked in the line's
/// color too.
///
/// The returned handle is the channel: it must be kept alive until the
/// script has taken the payload, so callers hold it for the life of the
/// screen. It also carries back, as [`TrackMapEvent`]s, a photo tapped in a
/// popup (US-62) and a waypoint or a linked line chosen (US-104).
pub fn start_track_map(
    line: OwnTrack,
    markers: Vec<PhotoMarker>,
    waypoints: Vec<MapWaypoint>,
    overlays: Overlays,
) -> document::Eval {
    start(
        &with_overlays(TRACK_MAP_SCRIPT),
        TrackMapView {
            line,
            markers,
            waypoints,
            overlays,
        },
        "the track map",
    )
}

/// Mark `at` on the track map, or clear the mark when there is nothing to
/// mark — the position Rust resolved the hovered chart index to (US-59).
///
/// Sent on the map's own channel, which the script keeps reading after it has
/// drawn: this is the map end of the round trip US-52's spike proved `eval`
/// carries (ADR-0025).
pub fn mark_on_track_map(map: &document::Eval, at: Option<[f64; 2]>) {
    if let Err(err) = map.send(at) {
        dioxus::logger::tracing::error!("could not mark the hovered point: {err}");
    }
}

/// Run a drawing script and hand it its payload over the channel — never
/// spliced into the script text, which is the injection bug the whole
/// mechanism is shaped to make impossible (ADR-0025).
pub(super) fn start(script: &str, payload: impl serde::Serialize, what: &str) -> document::Eval {
    let eval = document::eval(script);
    if let Err(err) = eval.send(payload) {
        dioxus::logger::tracing::error!("could not draw {what}: {err}");
    }
    eval
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::Kind;
    use crate::photos::PopupTap;

    #[test]
    fn us104_the_map_reports_a_photo_a_waypoint_and_a_linked_line_apart() {
        let read = |json| serde_json::from_value::<TrackMapEvent>(json).unwrap();

        assert_eq!(
            read(serde_json::json!({ "photo": { "marker": 1, "photo": 2 } })),
            TrackMapEvent::Photo(PopupTap {
                marker: 1,
                photo: 2
            })
        );
        assert_eq!(
            read(serde_json::json!({ "open_waypoint": 4 })),
            TrackMapEvent::OpenWaypoint(4)
        );
        assert_eq!(
            read(serde_json::json!({ "open_overlay": 0 })),
            TrackMapEvent::OpenOverlay(0)
        );
    }

    #[test]
    fn us104_the_pages_own_track_reaches_the_map_named_with_its_icon() {
        let view = TrackMapView {
            line: OwnTrack {
                name: "Albula".to_string(),
                color: "#b2182b",
                icon: None,
                dash: None,
                points: vec![[46.5, 9.8]],
            },
            markers: Vec::new(),
            waypoints: Vec::new(),
            overlays: Overlays::default(),
        };

        let sent = serde_json::to_value(&view).unwrap();

        assert_eq!(sent["name"], "Albula");
        assert_eq!(sent["color"], "#b2182b");
        assert_eq!(sent["points"], serde_json::json!([[46.5, 9.8]]));
        assert!(sent["icon"].is_null());
        assert!(sent["dash"].is_null(), "solid unless told otherwise");
    }

    #[test]
    fn us108_a_planned_trips_own_track_reaches_the_map_dashed() {
        let view = TrackMapView {
            line: OwnTrack {
                name: "Albula".to_string(),
                color: "#b2182b",
                icon: None,
                dash: Kind::Planned.dash(),
                points: vec![[46.5, 9.8]],
            },
            markers: Vec::new(),
            waypoints: Vec::new(),
            overlays: Overlays::default(),
        };

        let sent = serde_json::to_value(&view).unwrap();

        assert_eq!(sent["dash"], "8 6");
    }
}
