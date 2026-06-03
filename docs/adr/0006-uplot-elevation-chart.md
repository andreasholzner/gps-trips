# ADR-0006 — uPlot for the elevation chart

## Status

Accepted

## Context

The trip detail page needs an elevation profile: a line chart of elevation against cumulative
distance (or time). Options ranged from hand-drawing on a `<canvas>` via `web_sys`, to a
Rust charting crate (`plotters` + `plotters-canvas`), to wrapping a JS charting library.
Hand-rolling axes, ticks, tooltips, and hover interactions is avoidable work.

## Decision

Wrap **uPlot** (~40 KB) using the same vendored-JS + glue-module + wasm-bindgen pattern as
Leaflet ([ADR-0005](./0005-leaflet-osm-via-wasm-interop.md)). The chart's x/y series (cumulative
distance and elevation) are emitted into the track GeoJSON `properties` at import time, so the
client gets map geometry and chart data from a single fetch.

## Consequences

- Very small, fast charting with built-in axes/tooltips; handles tens of thousands of points.
- One more small vendored JS dependency, but reuses the established interop pattern (no new
  architecture).
- uPlot's hover callback exposes the data index, enabling an optional map↔chart hover-sync later.
