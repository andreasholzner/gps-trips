# ADR-0024 — Dioxus for the UI: a CSR SPA on the web and an Android app from one source

## Status

Accepted — supersedes [ADR-0001](./0001-rust-leptos-fullstack.md)

## Context

[ADR-0001](./0001-rust-leptos-fullstack.md) chose Leptos with SSR + hydration, orchestrated by
`cargo-leptos`. It was never implemented: the UI that exists is a deliberate proof of concept —
server-rendered HTML plus vanilla JS — built to validate the import pipeline, Komoot sync,
QMapShack export and the DB model, and always intended to be replaced.

Two decisions since then narrowed the field:

- **2026-07-31 —** the UI shape was settled as a **client-side-rendered SPA against the JSON
  API**, served as static files; SSR + hydration was abandoned. The framework was deliberately
  left open, to be decided by timeboxed spikes of the real trip-list screen against the real API.
- **[ADR-0023](./0023-managed-scale-to-zero-hosting.md) —** mobile access is the reason the UI
  question became urgent. It settled on the responsive web UI, with "a native app remains an
  option only if its effort stays modest".

The owner's stated motivation for looking at Dioxus at all was that it claims to target mobile
from the same source. A spike was run to test exactly that claim
(see [docs/dioxus-spike.md](../dioxus-spike.md)): the trip list and trip detail screens — filters,
tabs, the Leaflet map, the uPlot elevation profile, the photo gallery — built once and run both
as a web SPA and as an Android app. **Confirmed on the owner's own phone on 2026-08-26**: the
list, filtering, the detail screen, the map including pinch-zoom, in-app back navigation and
full-size photos all work. The Android build succeeded on the first attempt, and the component
code needed *no* platform-specific code at all.

**A Leptos spike was never run.** The decision does not rest on a head-to-head comparison of
web-side ergonomics; it rests on a capability Leptos does not offer at all. Leptos targets the
web. Given that mobile access is the motivating requirement, no web-side advantage it might have
shown would have changed the outcome — so the remaining spike was judged not worth its cost.

## Decision

Build the UI with **Dioxus 0.7** — one crate, two platforms:

- **Web:** a client-side-rendered WASM SPA (`dioxus/web`), built by the `dx` CLI and served as
  **static files by Axum under `/app`**, with an index fallback so the SPA's own routes survive a
  reload or a shared link. No SSR, no hydration, no `cargo-leptos`. The
  "binary + adjacent `public/`" deployable unit of
  [ADR-0016](./0016-assets-relative-to-executable.md) is preserved.
- **Android:** the same source built with `dioxus/mobile`, which renders through an Android
  System WebView (wry). Requires an explicit `--target aarch64-linux-android`; the default
  produces an x86_64 emulator build.
- **iOS is out of scope** and untested — it needs macOS and Xcode, neither of which the owner has.

Supporting decisions that come with it:

- **The shared data models live in their own crate**, free of server dependencies, so the same
  types compile for the server, for `wasm32-unknown-unknown` and for Android. The SQLite
  mappings sit behind an optional feature only the server enables.
- **JS interop must work on both platforms.** Typed `wasm-bindgen` externs are a wasm-only
  mechanism and cannot exist on a webview platform, so the UI is confined to interop that is
  portable. Which mechanism that leaves, and the rules that come with it, are decided by
  [ADR-0025](./0025-js-widget-interop-via-eval.md).
- **The JSON API ([ADR-0008](./0008-json-first-api.md)) is the only contract** between UI and
  server. It is no longer a hedge for a hypothetical future client — it is the load-bearing
  interface for both shipped UIs.
- **Dioxus crate and `dx` CLI versions must match** (0.7.10 today) and are pinned together.

## Consequences

- **US-16 changes shape.** "PWA first, native app possible later" becomes "a native Android app
  is a build target of the existing UI". The responsive web UI still exists and still works, so
  the PWA route remains available; it is no longer the only route.
- **The server-rendered PoC UI is now legacy.** It keeps serving `/` until the SPA reaches
  parity — editing, tagging, bulk-tag, region filter, import and Komoot sync are all still
  PoC-only. Retiring it needs its own plan and stories; this ADR does not schedule it, and
  [ADR-0012](./0012-tdd-test-strategy.md)'s migration rule constrains it — a page stays until the
  acceptance assertions riding on it have moved to the SPA.
- **[ADR-0005](./0005-leaflet-osm-via-wasm-interop.md) and
  [ADR-0006](./0006-uplot-elevation-chart.md) are partly invalidated.** Their library choices
  (Leaflet + OSM raster tiles, uPlot) stand and are proven on both platforms. Their *mechanisms*
  do not: the `#[wasm_bindgen(module = …)]` glue module, the Leptos `NodeRef` + `Effect`, and the
  "must never run during SSR" constraint all describe an architecture that no longer exists.
  [ADR-0025](./0025-js-widget-interop-via-eval.md) supersedes both, keeping their library choices
  and replacing the mechanism.
- **[ADR-0015](./0015-db-model-response-type-separation.md) needed a decision, and has one.**
  Response types — a stored record plus per-request computed fields — lived with the HTTP handlers
  and could not be shared with a WASM or Android client, so the UI mirrored them by hand.
  ADR-0015's 2026-08-28 amendment moves them to the shared crate; the alternative of generating
  them from a schema was considered and rejected as disproportionate.
- **Auth ([ADR-0010](./0010-single-user-optional-auth.md), US-19) now gates two clients.** It was
  already a blocking prerequisite for exposing the instance; the mobile app makes it more urgent,
  since a phone on an untrusted network is the normal case rather than the exception.
- **HTTPS is effectively required for the mobile app.** The WebView serves the app from
  `https://dioxus.localhost`, so photo thumbnails fetched over plain `http://` are blocked as
  mixed content. Against the deployed HTTPS instance of ADR-0023 this resolves itself; against a
  plain-HTTP laptop it does not.
- **A breaking API change is a two-part release.** The web client ships with the server, but the
  Android app is installed and can lag the deployed instance indefinitely. Changing the API is
  therefore a server deploy *plus* an app rebuild and reinstall — acceptable, since every deployment
  target is the owner's, but nothing in the test suite reports a skipped second half. The symptom is
  a broken app on the phone and a green build.
- **Interop is untyped forever.** Every JS library the UI touches is reached through strings with
  no compile-time checking and errors that surface in a webview console — awkward to read on a
  phone. This is the concrete price of the multi-platform property.
- **[ADR-0023](./0023-managed-scale-to-zero-hosting.md)'s open condition is answered.** It kept a
  native app as an option "only if its effort stays modest"; the spike showed it costs one build
  target of a UI that has to exist anyway, so the condition is met rather than waived.
- **[ADR-0008](./0008-json-first-api.md)'s references to a Leptos UI are historical**, not a live
  constraint; the decision it records — plain Axum JSON handlers as the contract — is unchanged
  and now carries more weight than when it was made.
- **The living documents follow this decision:** `architecture.md`'s UI containers and components
  now describe the Dioxus target rather than the Leptos one, and `requirements.md` re-traces US-7
  to the interop ADR that replaces ADR-0005/0006 and re-states US-16 as a native app built from
  the same source.
- **Web-side ergonomics were not competitively evaluated.** Dioxus's were judged good enough on
  their own merits — fast RSX hot reload, an undramatic signals/resources model, pure logic
  testable on the host target — but "good enough" here is an absolute judgement, not a
  comparative one. If the web UI later proves frustrating, this ADR is the record that the
  alternative was never measured.
