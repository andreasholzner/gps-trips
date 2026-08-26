# ADR-0025 — Vendored JS widgets driven from Rust through `document::eval`

## Status

Accepted — supersedes [ADR-0005](./0005-leaflet-osm-via-wasm-interop.md) and
[ADR-0006](./0006-uplot-elevation-chart.md)

## Context

Two ADRs chose the client-side widgets the trip detail screen needs: **Leaflet** with OSM raster
tiles for the map (ADR-0005) and **uPlot** for the elevation profile (ADR-0006). Those library
choices were sound and are not in question here.

Their *mechanism* is. Both specified a Leptos-and-wasm-bindgen design: a glue ES module wrapping
each library behind a small named API, bound from Rust with `#[wasm_bindgen(module = …)]`,
initialized from a Leptos component after mount, with the container rendered empty during SSR.
ADR-0006 has no mechanism of its own at all — its decision is explicitly "the same vendored-JS +
glue-module + wasm-bindgen pattern as ADR-0005".

Three things have since made that description false:

- **[ADR-0024](./0024-dioxus-ui-web-and-android.md) replaced the framework.** There is no Leptos,
  no `NodeRef`, no `Effect`, and no SSR — so the "must never run during SSR" constraint that
  shaped the original design no longer has anything to constrain.
- **The UI now also runs on Android**, where the Rust is compiled natively and driving a WebView.
  `#[wasm_bindgen]` externs are a wasm-only mechanism; on that platform they cannot exist. Any
  interop that is not portable would mean maintaining two implementations of every widget.
- **The glue module was never built.** It exists in no commit; the PoC UI drives Leaflet and
  uPlot directly from ordinary page scripts. ADR-0005 documents an architecture that was never
  implemented, in the same way ADR-0001 did.

The Dioxus spike (see [docs/dioxus-spike.md](../dioxus-spike.md)) deliberately implemented both
interop styles side by side — Leaflet through `document::eval`, uPlot through typed
`wasm-bindgen` externs — to compare them. The typed style was better to write and read, and is
compile-time checked. It is also unavailable off the web, so it was removed.

## Decision

**Keep the libraries; replace the mechanism.**

Leaflet with OSM raster tiles, and uPlot, remain the map and chart, vendored and self-hosted
(US-10) with no external CDN or API key. The chart's series (cumulative distance and elevation)
continue to be emitted into the track GeoJSON `properties` at import time, so map geometry and
chart data still arrive in a single fetch.

Both are driven from **a single interop module** in the UI crate using **`document::eval`** — a JS source string plus a message channel — because it is the only interop
that works identically on the web and on Android. Specifically:

- **No `wasm-bindgen` externs and no `web-sys` DOM manipulation in UI code.** Neither exists on
  the mobile target, so using either would fork the codebase by platform.
- **Payloads cross over `dioxus.recv()`, never by interpolation into the script source.**
  Splicing a server value into JS text is the injection bug the server-rendered pages avoid with
  `html_escape`; the channel makes it structurally impossible.
- **Scripts wait for the library's global rather than assuming load order.** The libraries are
  injected as `document::Script` tags and load asynchronously, so a widget that assumed
  availability would work on a slow network and fail on a fast one.
- **Each widget draws into a container Dioxus never re-renders with children, and guards against
  drawing twice.** A JS library owns its subtree outright; the virtual DOM must not compete for it.
- **JS renders; Rust decides.** The scripts contain no fetching and no business logic — Rust
  fetches over the JSON API ([ADR-0008](./0008-json-first-api.md)), prepares the values, and
  passes them in. This keeps the untyped surface as small as possible and the logic testable.
- **Libraries are bundled with `asset!`** so they ship inside the APK, where there is no server
  to fetch them from.

## Consequences

- One mechanism, one module, both platforms — no per-platform widget implementations.
- **The interop surface is not type-checked.** Mistakes surface in a webview console, which is
  awkward to read on a phone. Keeping the scripts small and rendering-only is the mitigation, and
  it is a deliberate trade for portability rather than an oversight.
- The Rust-side data preparation (parsing the GeoJSON `properties` arrays, unit conversion) is
  ordinary Rust and testable on the host target; only the drawing call is opaque.
- **The vendored libraries exist in two copies** — one bundled with the UI crate, one served to
  the PoC UI — because `asset!` refuses paths outside its own crate, symlinks included. This
  resolves itself when the PoC UI is retired.
- Switching tile provider, or self-hosting tiles, remains a change inside one script.
- Photo map markers (US-3/US-4) are not yet in the SPA. When they arrive they follow these rules:
  markers are added to the same map instance from prepared Rust values.
- **Revisit if a widget needs sustained two-way interaction.** `eval`'s send-and-forget shape
  suits "draw this"; a map that must continuously report clicks, drags and viewport changes back
  into Rust state would strain it. The map↔chart hover-sync ADR-0006 anticipated is exactly such
  a case, and should be treated as a trigger to re-examine this decision rather than something to
  force through `eval`.
- If a mature native-Rust map renderer ever appears, the JS dependency could be dropped entirely —
  the reason ADR-0005 gave for needing JS at all still holds today.
