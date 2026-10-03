//! The statistics screen's two charts (US-77): one activity's totals as
//! bars, and every year's running total as lines. Rust adds the figures up
//! (`crate::stats`); these scripts only draw them (ADR-0025).
//!
//! Both are redrawn whenever a control changes, so — like the elevation
//! chart — each keeps its instance in the page's widget registry and
//! destroys the previous one before drawing the next into its container.

use dioxus::prelude::*;
use serde::Serialize;

/// Waiting for uPlot and the container, the registry, and the page's own
/// colours for the axes — shared by both scripts. uPlot draws into a canvas
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
    const muted = () => pico("--pico-muted-color");
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

/// The bars: one per column, labelled with the column's name, from zero up.
const BARS_SCRIPT: &str = r##"
    const CONTAINER = "stats-bars";
    PRELUDE
    if (!view.values.length) return;

    const xs = view.values.map((_, i) => i);
    widgets[CONTAINER] = new uPlot(
      {
        width: el.clientWidth || 600,
        height: 220,
        legend: { show: false },
        cursor: { drag: { x: false, y: false } },
        scales: {
          x: { time: false, range: () => [-0.5, xs.length - 0.5] },
          y: { range: (u, min, max) => [0, max > 0 ? max : 1] },
        },
        series: [
          {},
          {
            label: view.label,
            stroke: view.color,
            fill: view.color,
            paths: uPlot.paths.bars({ size: [0.7, 64] }),
            points: { show: false },
          },
        ],
        axes: [
          {
            ...themed,
            grid: { show: false },
            splits: () => xs,
            values: (u, splits) => splits.map((i) => view.labels[i] ?? ""),
          },
          { label: view.label, ...themed },
        ],
      },
      [xs, view.values],
      el,
    );
"##;

/// The lines: one per year against the day of the year, the highlighted
/// year in the accent colour and the others muted. The legend names the
/// years and reads their totals at the cursor, and its entries switch a
/// year's line off and on.
const RUNNING_SCRIPT: &str = r##"
    const CONTAINER = "stats-running";
    PRELUDE
    if (!view.years.length) return;

    const xs = view.day_labels.map((_, i) => i);
    widgets[CONTAINER] = new uPlot(
      {
        width: el.clientWidth || 600,
        height: 260,
        scales: {
          x: { time: false },
          y: { range: (u, min, max) => [0, max > 0 ? max : 1] },
        },
        series: [
          { label: "Date", value: (u, i) => (i == null ? "–" : view.day_labels[i]) },
          ...view.years.map((year) =>
            year === view.highlighted
              ? { label: String(year), stroke: view.color, width: 2.5 }
              : { label: String(year), stroke: muted, width: 1 },
          ),
        ],
        axes: [
          {
            ...themed,
            splits: () => view.month_starts,
            values: (u, splits) => splits.map((_, i) => view.month_labels[i] ?? ""),
          },
          { label: view.label, ...themed },
        ],
      },
      [xs, ...view.series],
      el,
    );
"##;

/// What the bar chart draws.
#[derive(Serialize)]
struct BarsView {
    labels: Vec<String>,
    values: Vec<f64>,
    color: &'static str,
    label: &'static str,
}

/// Draw `values` as bars into `#stats-bars`, one per label, in `color`, on
/// an axis called `label`. The handle must be kept until the script has
/// taken the payload.
pub fn draw_stats_bars(
    labels: Vec<String>,
    values: Vec<f64>,
    color: &'static str,
    label: &'static str,
) -> document::Eval {
    start(
        BARS_SCRIPT,
        BarsView {
            labels,
            values,
            color,
            label,
        },
    )
}

/// What the running-total chart draws.
#[derive(Serialize)]
pub struct RunningView {
    pub years: Vec<i32>,
    /// Per year, a value per day of the year; `null` where the line stops.
    pub series: Vec<Vec<Option<f64>>>,
    pub highlighted: i32,
    pub color: &'static str,
    pub label: &'static str,
    /// The name of each day of the year, for the cursor's readout.
    pub day_labels: Vec<String>,
    /// Where each month starts among those days, and its name, for the ticks.
    pub month_starts: Vec<usize>,
    pub month_labels: Vec<&'static str>,
}

/// Draw the running totals into `#stats-running`, on the same terms as
/// [`draw_stats_bars`].
pub fn draw_stats_running(view: RunningView) -> document::Eval {
    start(RUNNING_SCRIPT, view)
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
