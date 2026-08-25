# Dioxus spike — trip list & trip detail

**Date:** 2026-08-25 · **Branch:** `spike-dioxus` · **Status:** spike complete, verdict pending
the Leptos spike.

A timeboxed spike of the two core screens as a client-side-rendered Rust/WASM SPA against the
existing JSON API, to inform the UI-framework decision that will re-decide
[ADR-0001](./adr/0001-rust-leptos-fullstack.md). The motivation for looking at Dioxus at all
is its multi-platform story (a future Android app from the same components, US-16).

**No ADR is written or changed by this spike.** The decision belongs to a planning step once
both spikes are in.

## What was built

| | |
|---|---|
| Crate | `crates/ui-dioxus` (781 lines of Rust across 7 files) |
| Framework | `dioxus` 0.7.10 (`web` + `router`), `dx` CLI 0.7.10 |
| Data | the real API — `GET /api/trips`, `/api/trips/:id`, `/api/trips/:id/photos`, `/api/trips/:id/track.geojson` |
| Screens | trip list (US-6/13/32) and trip detail (US-7) |

**Trip list** — Recorded/Planned tabs, free-text search, activity/date/distance filters
(re-querying as you type), empty states, rows linking to the detail screen.

**Trip detail** — stats, the track on a Leaflet/OSM map, the uPlot elevation profile, and the
photo gallery, each fetched independently so one failure can't blank the others.

Deliberately out of scope: styling, editing, tagging/bulk-tag, region filter, import, Komoot
sync, auth, photo map markers, and any mobile build.

### Two changes outside the spike crate

Both are prerequisites for *any* Rust WASM UI, not Dioxus-specific, and both are covered by
tests:

1. **`crates/types` (`trip-archive-types`)** — `src/models/` moved into its own crate so the
   same types serve the server and compile to `wasm32-unknown-unknown`. The `sqlx::Type`
   derives moved behind an optional `sqlx` feature that only the server enables. The server
   crate re-exports it (`pub use trip_archive_types as models`), so every existing
   `crate::models::…` path is unchanged.
2. **`GET /api/trips/:id`** (`tests/us16_trip_detail_api.rs`) — the detail screen had no JSON
   endpoint; only the server-rendered HTML page existed.

Plus `/app` serving the built bundle from `public/app` with an index fallback for deep links
(`tests/spike_spa_bundle.rs`). The server-rendered PoC UI at `/` is untouched and still
tested.

## Running it

```bash
# Terminal 1 — the API (any data dir)
cargo run --bin trip-archive

# Terminal 2 — the SPA dev server, proxying /api, /media and /static to it
cd crates/ui-dioxus && dx serve --platform web        # http://127.0.0.1:8080/app/

# Or the deployed shape: build, install into public/, serve from Axum itself
cd crates/ui-dioxus && dx build --release --platform web
cp -r target/dx/ui-dioxus/release/web/public public/app     # from the repo root
# → http://127.0.0.1:3000/app/
```

`dx` needs no source build: the prebuilt `dx-x86_64-unknown-linux-gnu.tar.gz` from the
[v0.7.10 release](https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.10) plus
`rustup target add wasm32-unknown-unknown` is the whole toolchain setup.

## Verified

Driven end-to-end in a headless browser, against both the dev server and the release bundle
served by Axum, with **no console errors** in either:

- list renders and filters (typing `inn` narrows to *Inn Valley Ride*); tab switch shows the
  planned trip
- client-side navigation to the detail screen; stats, 10 OSM tiles, the track path, the uPlot
  canvas with both series labelled, and two gallery thumbnails from `/media`
- deep link straight to `/app/trips/1` (a reload, not client-side navigation) renders the
  same

## Measurements

| | |
|---|---|
| Cold build (debug) | 98 s |
| Cold build (release) | 79 s |
| Incremental rebuild, non-RSX change | 9.4 s (≈6 s of it `wasm-bindgen`) |
| RSX-only text change | **hot-reloaded in ~1 s, no rebuild** |
| Release bundle, wasm | 2.0 MB raw / **523 KB gzipped** |
| Release bundle, JS glue | 58 KB raw / 14 KB gzipped |

8-core machine, Rust 1.96.1. Rust hot-*patching* (`dx serve --hot-patch`) was not tried; it is
flagged experimental.

## Findings

### Good

- **Rendering and routing were undramatic.** `use_signal` + `use_resource` cover the whole
  list screen: reading the filter signal inside the resource closure *is* the subscription —
  no dependency array to keep in sync. The `Routable` derive plus `Link` gave client-side
  navigation and working deep links with no extra wiring, and it picked up the `/app` base
  path from `Dioxus.toml` on its own.
- **RSX hot reload is genuinely fast** and is where most UI iteration time goes. Anything
  touching Rust logic drops to a ~9 s rebuild, so the two modes feel very different.
- **The whole crate also compiles for the host target**, so `cargo test -p ui-dioxus` runs the
  pure logic (formatting, query building — 9 tests) natively with no wasm test runner. That
  was not a given.
- **`dx` needed no configuration beyond a 20-line `Dioxus.toml`** — proxy, base path, title.

### Friction

- **Interop is where the real work is, and it stays your problem.** Neither approach is
  supported by the framework beyond the raw mechanism (see the comparison below).
- **A JS library owns its DOM subtree.** The map and chart containers must be elements Dioxus
  never re-renders with children, and both draws need a guard against running twice (the
  photos resource resolving later re-runs the effect). This is inherent to VDOM-plus-JS, not
  a Dioxus flaw, but it is the thing most likely to bite later.
- **`#[component]` requires `PartialEq` on every prop type**, which meant adding
  `PartialEq` derives to `TripSummary` and `TripDetail` in the shared crate. Harmless here;
  worth knowing that component props constrain shared server types.
- **`serde-wasm-bindgen`'s default serializes structs to JS `Map`s**, which any plain-object
  JS API reads as an empty options object — silently. `Serializer::json_compatible()` is
  required. Cost 20 minutes; would have cost longer without a known-good JS reference to
  compare against.
- **Build output is not small.** 523 KB gzipped for two screens is roughly ten times the
  vanilla-JS PoC's payload. Fine over broadband, less so as the "mobile-first" story, and it
  is mostly framework floor rather than app code.

### The two interop styles, compared

Both are in `crates/ui-dioxus/src/interop.rs`, deliberately.

| | `document::eval` (Leaflet) | `wasm-bindgen` externs (uPlot) |
|---|---|---|
| Setup | none — a JS string | one `extern "C"` block per entry point |
| Type checking | none; typos surface in the browser console | call sites checked at compile time |
| Passing data | `dioxus.recv()` channel, serde-serialized | typed args, `serde_wasm_bindgen` |
| Reading results back | `await` the eval handle | direct return values |
| Reads like | JS embedded in Rust | Rust |

For a one-shot "draw this and forget it" widget, `eval` is less code and hard to get wrong.
For anything the Rust side keeps talking to — a map instance that gains photo markers later,
responds to clicks, or gets re-fitted when a filter changes — the typed bindings are clearly
where you want to end up, and the ~15 lines they cost per entry point are cheap. **A real
implementation should use wasm-bindgen bindings for Leaflet and keep `eval` for throwaways.**
The string-splicing injection risk in `eval` is also real: the GeoJSON is passed over the
channel rather than interpolated into the script text for exactly that reason.

### Framework-independent findings (they apply to Leptos too)

- The **types crate split works and is cheap** — the server needed one changed line.
- **Response types are not shareable.** `PhotoResponse` (`Photo` + per-request `url` /
  `thumbnail_url`, per ADR-0015) lives in `http.rs`, so the SPA mirrors it by hand in
  `api.rs`. That is the one duplicated shape in the crate, and it will multiply as the SPA
  grows. Before committing to a Rust SPA, decide where response types live: moved into the
  types crate, or generated. This is a real ADR-0015 question, not a spike detail.
- **The `/app` + index-fallback shape works** and keeps the "binary + adjacent `public/`"
  deployable unit of ADR-0016 intact — nothing new to configure at deploy time.
- **A CSR SPA needs the API to be complete.** One screen already exposed a missing endpoint;
  editing, tagging, and import will each need the same audit.

## What this does not tell us

- **Nothing about mobile was measured.** Per the spike's scope decision, no Android build was
  attempted. Dioxus's `mobile` feature exists and the component code has no web-specific types
  in it — except `interop.rs`, which is *entirely* web-specific. On a native mobile target
  there is no Leaflet and no uPlot, so the map and chart would need replacing outright. That
  materially weakens "the same components run on Android": the parts that port cleanly are the
  list, the stats, and the gallery; the two most valuable widgets do not port at all.
  **If mobile is the reason to choose Dioxus, this is the question the next spike should
  answer**, not framework ergonomics.
- Nothing about styling, accessibility, or bundle-splitting.
- No component-level test story was evaluated; only pure logic is covered by tests here.

## For a fair Leptos comparison

Build the same two screens, against the same API, and record the same numbers: cold build,
incremental rebuild, hot-reload behaviour, gzipped wasm, and — most importantly — the same
two interop styles for Leaflet and uPlot. The types crate is already in place and framework-
agnostic, so a Leptos spike starts from step 3 of this one.

## Cleanup

The spike is additive and reversible. Dropping it means deleting `crates/ui-dioxus`, the
`/app` route + `paths::spa_dir`, and `tests/spike_spa_bundle.rs`. Keep `crates/types` and
`GET /api/trips/:id` either way — both are wanted by any SPA and by US-16.
