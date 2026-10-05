//! The statistics screen's chart (US-77): the totals per year or month, as
//! a line per activity. Rust works out every figure (`crate::stats::plot`);
//! this script only draws them (ADR-0025).
//!
//! It is redrawn whenever a control changes, so — like the elevation chart
//! — it keeps its instance in the page's widget registry and destroys the
//! previous one before drawing the next into its container.

use dioxus::prelude::*;
use serde::Serialize;

use crate::stats::Plot;

/// Waiting for uPlot and the container, the registry, and the page's own
/// colours for the axes. uPlot draws into a canvas
/// CSS cannot reach, so the colours are read from Pico's variables, through
/// functions uPlot calls on every draw, and a scheme change repaints.
const PRELUDE: &str = r##"
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
    const view = await dioxus.recv();
    if (widgets[CONTAINER]) {
      widgets[CONTAINER].destroy();
      widgets[CONTAINER] = null;
    }

    const pico = (name) => getComputedStyle(el).getPropertyValue(name).trim();
    const text = () => pico("--pico-color");
    const line = () => pico("--pico-muted-border-color");
    const themed = { stroke: text, ticks: { stroke: line }, grid: { stroke: line } };

    const listener = CONTAINER + "ThemeListener";
    if (!widgets[listener]) {
      widgets[listener] = () => widgets[CONTAINER]?.redraw(false, true);
      window
        .matchMedia("(prefers-color-scheme: dark)")
        .addEventListener("change", widgets[listener]);
    }
"##;

/// A point per column on each series' line, labelled with the column's
/// name, from zero up: the activities in their colors, the activities
/// together a dashed line in the text color. A line breaks where a series
/// has no figure. The legend names the series and reads each one's value at
/// the cursor, as the table shows it.
///
/// The column names thin out where they would collide on a phone.
const PLOT_SCRIPT: &str = r##"
    const CONTAINER = "stats-plot";
    PRELUDE
    if (!view.series.length) return;

    const xs = view.labels.map((_, i) => i);
    const styled = (series) => {
      const stroke = series.color ?? text;
      return {
        label: series.label,
        stroke,
        width: 2,
        dash: series.color ? undefined : [6, 4],
        points: { show: true, size: 8, fill: stroke },
        value: (u, v, s, i) => (i == null ? "–" : series.shown[i]),
      };
    };
    widgets[CONTAINER] = new uPlot(
      {
        width: el.clientWidth || 600,
        height: 240,
        cursor: { drag: { x: false, y: false } },
        scales: {
          x: { time: false, range: () => [-0.5, xs.length - 0.5] },
          y: { range: (u, min, max) => [0, max > 0 ? max : 1] },
        },
        series: [
          { label: view.column, value: (u, i) => (i == null ? "–" : view.labels[i]) },
          ...view.series.map(styled),
        ],
        axes: [
          {
            ...themed,
            grid: { show: false },
            splits: () => xs,
            values: (u, splits) => {
              const every = Math.max(1, Math.ceil((xs.length * 40) / (u.width || 600)));
              return splits.map((i) => (i % every ? "" : view.labels[i] ?? ""));
            },
          },
          { label: view.axis, ...themed },
        ],
      },
      [xs, ...view.series.map((series) => series.drawn)],
      el,
    );
"##;

/// Draw `plot` into `#stats-plot`. The handle must be kept until the script
/// has taken the payload.
pub fn draw_stats_plot(plot: Plot) -> document::Eval {
    start(PLOT_SCRIPT, plot)
}

/// Run a script with the shared prelude, handing it its payload over the
/// channel — never spliced into the script (ADR-0025).
fn start(script: &str, payload: impl Serialize) -> document::Eval {
    let eval = document::eval(&script.replace("PRELUDE", PRELUDE));
    if let Err(err) = eval.send(payload) {
        dioxus::logger::tracing::error!("could not draw a statistics chart: {err}");
    }
    eval
}
