//! The detail screen's elevation profile (US-7), beside the track map: the
//! series Rust has already prepared (`crate::track`) — this script only
//! draws them, and reports the point the cursor is on (US-59).

use dioxus::prelude::*;
use serde::Serialize;

use super::track::start;

/// The elevation profile. Draws into `#elevation`: elevation in metres
/// against cumulative distance in kilometres, the pair of series Rust
/// prepared — and, for a track recorded with times, the speed in km/h
/// against an axis of its own on the right (US-79). Every series shares the
/// one distance scale, which is what makes a drag zoom them all.
///
/// Unlike the map there is nothing to show before the payload arrives — an
/// empty chart frame is not a useful thing to look at — so this one waits
/// for its series first and draws then.
///
/// Redrawn and disposed of on the same terms as the track map: a chart is
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

    const {
      distance_km: distanceKm,
      elevation_m: elevationM,
      speed_kmh: speedKmh,
      climbs,
    } = await dioxus.recv();
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
        // Speed from zero up: a profile that floats its baseline would make a
        // slow trip look as varied as a fast one.
        scales: {
          x: { time: false },
          m: {},
          kmh: { range: (u, min, max) => [0, max > 0 ? max : 1] },
        },
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
        // A planned trip has no times, so no speed: its chart is the
        // elevation profile alone, with no right axis claiming a series that
        // is not there. A point without a speed is `null`, which uPlot leaves
        // as a gap in the line.
        //
        // The climbs (US-81) come first, so the elevation line is drawn over
        // them: a fill from the line down to the axis, with no line of its
        // own and no cursor mark, broken where the series is `null`.
        series: [
          { label: "Distance (km)" },
          ...(climbs
            ? [
                {
                  label: "Climbs",
                  scale: "m",
                  stroke: "transparent",
                  width: 0,
                  fill: "rgba(51, 103, 214, 0.15)",
                  fillTo: (u) => u.scales.m.min,
                  points: { show: false },
                },
              ]
            : []),
          { label: "Elevation (m)", scale: "m", stroke: "#3367d6", width: 2 },
          ...(speedKmh ? [{ label: "Speed (km/h)", scale: "kmh", stroke: "#e8710a", width: 1.5 }] : []),
        ],
        // The right axis's text is themed like the left's rather than in the
        // series' orange: no orange reads on both the light and the dark page.
        // It draws no grid of its own, which would cross the elevation's.
        axes: [
          { label: "Distance (km)", ...themed },
          { label: "Elevation (m)", scale: "m", ...themed },
          ...(speedKmh
            ? [{ label: "Speed (km/h)", scale: "kmh", side: 1, ...themed, grid: { show: false } }]
            : []),
        ],
      },
      [distanceKm, ...(climbs ? [climbs] : []), elevationM, ...(speedKmh ? [speedKmh] : [])],
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

/// What the elevation chart draws: elevation against distance, and the speed
/// over the same distances where the track was recorded with times (US-79) —
/// `None` for one that was not, which draws no speed and no axis for it —
/// and the climbs' stretches to fill below the line (US-81), the elevation
/// on a climb and `null` elsewhere, or `None` for a trip without climbs.
#[derive(Serialize)]
struct ElevationChartView {
    distance_km: Vec<f64>,
    elevation_m: Vec<f64>,
    speed_kmh: Option<Vec<Option<f64>>>,
    climbs: Option<Vec<Option<f64>>>,
}

/// Start the elevation chart with its prepared series. The handle is the
/// channel, as the track map's is — and read in a loop: the chart reports the
/// index the cursor is on, or `null` when it leaves (US-59).
pub fn start_elevation_chart(
    distance_km: Vec<f64>,
    elevation_m: Vec<f64>,
    speed_kmh: Option<Vec<Option<f64>>>,
    climbs: Option<Vec<Option<f64>>>,
) -> document::Eval {
    start(
        ELEVATION_SCRIPT,
        ElevationChartView {
            distance_km,
            elevation_m,
            speed_kmh,
            climbs,
        },
        "the elevation chart",
    )
}
