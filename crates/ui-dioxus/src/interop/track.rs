//! The detail screen's two widgets (US-7): the track on an OSM map, and the
//! elevation profile. Both are handed values Rust has already prepared
//! (`crate::track`) — these scripts only draw.

use dioxus::prelude::*;
use serde::Serialize;

use crate::photos::PhotoMarker;

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
    for (const layer of [map.trackLine, map.photoMarkers, map.hoverMark]) {
      if (layer) map.removeLayer(layer);
    }
    map.trackLine = null;
    map.photoMarkers = null;
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
              dioxus.send({ marker: groupIndex, photo: photoIndex });
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
                fillColor: "#d6336c",
                fillOpacity: 1,
              });
        return marker.bindPopup(popup);
      }),
    ).addTo(map);

    const points = view.points || [];
    if (points.length > 0) {
      map.trackLine = L.polyline(points, { color: "#3367d6", weight: 3 }).addTo(map);
    }

    // Framed on everything there is to see. A track with no drawable
    // positions still has photos, and they should not be left somewhere on a
    // view of the whole world.
    const bounds = L.latLngBounds([]);
    if (map.trackLine) bounds.extend(map.trackLine.getBounds());
    if (map.photoMarkers.getLayers().length > 0) {
      bounds.extend(map.photoMarkers.getBounds());
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
          color: "#3367d6",
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

/// The elevation profile. Draws into `#elevation`: elevation in metres
/// against cumulative distance in kilometres, the pair of series Rust
/// prepared.
///
/// Unlike the map there is nothing to show before the payload arrives — an
/// empty chart frame is not a useful thing to look at — so this one waits
/// for its series first and draws then.
///
/// Redrawn and disposed of on the same terms as the map above: a chart is
/// replaced outright rather than updated in place, because a new trip's
/// series are a different length and uPlot's axes have to be rebuilt anyway.
const ELEVATION_SCRIPT: &str = r##"
    const CONTAINER = "elevation";

    async function ready() {
      for (let i = 0; i < 400; i++) {
        if (window.uPlot && document.getElementById(CONTAINER)) return true;
        await new Promise((r) => setTimeout(r, 25));
      }
      return false;
    }
    if (!(await ready())) return;

    const el = document.getElementById(CONTAINER);
    const widgets = (window.tripArchiveWidgets ||= {});

    const [distanceKm, elevationM] = await dioxus.recv();
    if (widgets[CONTAINER]) {
      widgets[CONTAINER].destroy();
      widgets[CONTAINER] = null;
    }
    if (!distanceKm || distanceKm.length === 0) return;

    // uPlot binds mouse events by name, and a finger sends none of them. Each
    // binding below registers the handler under its pointer equivalent and
    // returns nothing, so uPlot adds no listener of its own: a `PointerEvent`
    // is a `MouseEvent`, so the handler itself needs no changing. The
    // container also has to claim the gesture in CSS (`touch-action`), or a
    // drag along the chart is consumed as a page scroll and never arrives.
    //
    // `only` narrows a binding to one kind of pointer. `pointerleave` takes
    // it: a touch pointer is destroyed the moment the finger lifts, and
    // treating that as "the cursor left the chart" would clear a reading the
    // owner has only just taken. With a mouse the cursor really does leave,
    // and the reading goes with it.
    const asPointer = (name, only) => (u, target, handler) => {
      target.addEventListener(name, (event) => {
        if (only && event.pointerType !== only) return;
        handler(event);
      });
      return null;
    };

    // The axes in the page's own colours. uPlot draws them into its canvas,
    // where CSS cannot reach, in black unless told otherwise — unreadable on
    // the dark scheme. Functions rather than colours, because uPlot calls
    // them on every draw: a repaint after the scheme changes picks up the
    // new ones.
    const pico = (name) => getComputedStyle(el).getPropertyValue(name).trim();
    const text = () => pico("--pico-color");
    const line = () => pico("--pico-muted-border-color");
    const themed = { stroke: text, ticks: { stroke: line }, grid: { stroke: line } };

    // The index last reported to Rust. uPlot fires `setCursor` on every
    // pointer move; only a move onto a different sample is news.
    let reported;

    widgets[CONTAINER] = new uPlot(
      {
        width: el.clientWidth || 600,
        height: 200,
        scales: { x: { time: false } },
        // Off (US-59): clicking it toggles the series off and leaves an empty
        // frame, and its marker square reads as a checkbox. The live readout
        // it also carried is rendered as ordinary markup by Rust instead,
        // through the same formatting as the stats above the chart.
        legend: { show: false },
        cursor: {
          bind: {
            mousemove: asPointer("pointermove"),
            mouseleave: asPointer("pointerleave", "mouse"),
            mousedown: asPointer("pointerdown"),
            mouseup: asPointer("pointerup"),
          },
        },
        hooks: {
          setCursor: [
            (u) => {
              const idx = u.cursor.idx ?? null;
              if (idx === reported) return;
              reported = idx;
              dioxus.send(idx);
            },
          ],
        },
        series: [
          { label: "Distance (km)" },
          { label: "Elevation (m)", stroke: "#3367d6", width: 2 },
        ],
        axes: [
          { label: "Distance (km)", ...themed },
          { label: "Elevation (m)", ...themed },
        ],
      },
      [distanceKm, elevationM],
      el,
    );

    const chart = widgets[CONTAINER];
    const over = el.querySelector(".u-over");

    // Repaint when the colour scheme changes under an open chart. One
    // listener per page, reaching whichever chart is current, so redrawing
    // for another trip adds none.
    if (!widgets.elevationThemeListener) {
      widgets.elevationThemeListener = () => widgets[CONTAINER]?.redraw(false, true);
      window
        .matchMedia("(prefers-color-scheme: dark)")
        .addEventListener("change", widgets.elevationThemeListener);
    }

    // A tap places the cursor. uPlot only ever positions it on a move, and a
    // tap is a `pointerdown` and a `pointerup` with nothing in between, so
    // without this a tap reads nothing at all. `true` fires the hooks, which
    // is what sends the index on to Rust.
    over.addEventListener("pointerdown", (event) => {
      // Dragging across the chart zooms into that range — kept for the mouse,
      // where a double-click puts it back, and taken away from the finger,
      // which has no double-click and would be left zoomed in with no way
      // out. uPlot reads this at the end of the drag, so setting it per
      // gesture works.
      chart.cursor.drag.setScale = event.pointerType === "mouse";
      const bounds = over.getBoundingClientRect();
      chart.setCursor(
        { left: event.clientX - bounds.left, top: event.clientY - bounds.top },
        true,
      );
    });

    // A drag that does not zoom still draws uPlot's selection band, and with
    // nothing to commit it to, the band would simply stay there.
    over.addEventListener("pointerup", (event) => {
      if (event.pointerType === "mouse") return;
      chart.setSelect({ width: 0, height: 0 }, false);
    });
"##;

/// What the map shows: the track as `[lat, lon]` pairs, and a marker per
/// group of photos taken at the same place (US-3/US-4, grouped by US-57).
/// Sent as one value because it is one picture — a redraw with new markers
/// must not lose the line.
#[derive(Serialize)]
struct TrackMapView {
    points: Vec<[f64; 2]>,
    markers: Vec<PhotoMarker>,
}

/// Start the track map with the line and the photo markers to draw.
///
/// The returned handle is the channel: it must be kept alive until the
/// script has taken the payload, so callers hold it for the life of the
/// screen. It also carries a photo tapped in a popup back, as a
/// [`PopupTap`](crate::photos::PopupTap) (US-62).
pub fn start_track_map(points: Vec<[f64; 2]>, markers: Vec<PhotoMarker>) -> document::Eval {
    start(
        TRACK_MAP_SCRIPT,
        TrackMapView { points, markers },
        "the track map",
    )
}

/// Start the elevation chart with its two prepared series. The handle is the
/// channel, as above — and this one is read in a loop: the chart reports the
/// index the cursor is on, or `null` when it leaves (US-59).
pub fn start_elevation_chart(distance_km: Vec<f64>, elevation_m: Vec<f64>) -> document::Eval {
    start(
        ELEVATION_SCRIPT,
        (distance_km, elevation_m),
        "the elevation chart",
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
fn start(script: &str, payload: impl serde::Serialize, what: &str) -> document::Eval {
    let eval = document::eval(script);
    if let Err(err) = eval.send(payload) {
        dioxus::logger::tracing::error!("could not draw {what}: {err}");
    }
    eval
}
