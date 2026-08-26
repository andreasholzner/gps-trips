# Dioxus spike — trip list & trip detail, on web and Android

**Date:** 2026-08-25, Android confirmed on-device 2026-08-26 · **Branch:** `spike-dioxus` ·
**Status:** spike complete, verdict pending the Leptos spike.

A timeboxed spike of the two core screens as a Rust UI running on two platforms from one
source — a client-side-rendered WASM SPA on the web, and an Android app — both against the
existing JSON API. It informs the UI-framework decision that will re-decide
[ADR-0001](./adr/0001-rust-leptos-fullstack.md). The motivation for looking at Dioxus at all
is its multi-platform story (US-16).

**No ADR is written or changed by this spike.** The decision belongs to a planning step once
both spikes are in.

## What was built

|           |                                                                                                            |
|-----------|------------------------------------------------------------------------------------------------------------|
| Crate     | `crates/ui-dioxus` (968 lines of Rust across 8 files)                                                      |
| Framework | `dioxus` 0.7.10 (`router` + `asset`, with `web` / `mobile` per platform), `dx` CLI 0.7.10                  |
| Data      | the real API — `GET /api/trips`, `/api/trips/:id`, `/api/trips/:id/photos`, `/api/trips/:id/track.geojson` |
| Screens   | trip list (US-6/13/32), trip detail (US-7), and a settings screen for the API address                      |
| Platforms | web (`wasm32-unknown-unknown`) and Android (`aarch64-linux-android`) from the same source                  |

**Trip list** — Recorded/Planned tabs, free-text search, activity/date/distance filters
(re-querying as you type), empty states, rows linking to the detail screen.

**Trip detail** — stats, the track on a Leaflet/OSM map, the uPlot elevation profile, and the
photo gallery, each fetched independently so one failure can't blank the others.

**Settings** — where the archive lives. Invisible on the web (the SPA is served by the server
it queries); the first screen on Android, where nothing else can work until it is answered.

Deliberately out of scope: styling, editing, tagging/bulk-tag, region filter, import, Komoot
sync, auth, photo map markers, and iOS (needs macOS + Xcode).

### Changes outside the spike crate

Prerequisites for *any* Rust UI, not Dioxus-specific, and all covered by tests:

1. **`crates/types` (`trip-archive-types`)** — `src/models/` moved into its own crate so the
   same types serve the server and compile to `wasm32-unknown-unknown` and Android. The
   `sqlx::Type` derives moved behind an optional `sqlx` feature that only the server enables.
   The server crate re-exports it (`pub use trip_archive_types as models`), so every existing
   `crate::models::…` path is unchanged.
2. **`GET /api/trips/:id`** (`tests/us16_trip_detail_api.rs`) — the detail screen had no JSON
   endpoint; only the server-rendered HTML page existed.
3. **`/app`** serving the built web bundle from `public/app`, with an index fallback for deep
   links (`tests/spike_spa_bundle.rs`).

The server-rendered PoC UI at `/` is untouched and still tested.

## Running it

```bash
# Terminal 1 — the API
cargo run --bin trip-archive

# Terminal 2 — the SPA dev server, proxying /api and /media to it
cd crates/ui-dioxus && dx serve --platform web        # http://127.0.0.1:8080/app/

# The deployed web shape: build, install into public/, serve from Axum itself
cd crates/ui-dioxus && dx build --release --platform web
cp -r target/dx/ui-dioxus/release/web/public public/app     # from the repo root
# → http://127.0.0.1:3000/app/

# Android (needs the toolchain below)
cd crates/ui-dioxus && dx build --platform android --release --target aarch64-linux-android
# → target/dx/ui-dioxus/release/android/app/app/build/outputs/apk/debug/app-debug.apk
```

`--target aarch64-linux-android` is not optional: without it `dx` builds an **x86_64** APK for
an emulator, which installs on no phone. The output path says `debug` because that is Gradle's
variant name — the Rust inside is a release build, debug-*signed* so it can be sideloaded.

### Toolchain

`dx` itself needs no source build — `curl -fsSL https://dioxuslabs.com/install.sh | bash`, or
the prebuilt tarball from the v0.7.10 GitHub release. Then:

| Piece            | Notes                                                                                                                                                                                                                           |
|------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| JDK 17 + `unzip` | `openjdk-17-jdk-headless`. **17 specifically**: dx generates a Gradle 9.1.0 / AGP 8.7.0 / Kotlin 2.0.20 project with `jvmTarget = 17`. Gradle 9.1 itself runs on up to JDK 25, but AGP and Kotlin at those versions predate it. |
| Android SDK      | cmdline-tools, `platform-tools`, `platforms;android-34`, `build-tools;34.0.0` (dx defaults to `min_sdk 24` / `target_sdk 34`)                                                                                                   |
| NDK              | `ndk;27.3.13750724` (r27 LTS) — 2.0 GB of the ~2.4 GB total SDK footprint                                                                                                                                                       |
| Rust target      | `aarch64-linux-android`                                                                                                                                                                                                         |
| Env              | `ANDROID_HOME`, `ANDROID_NDK_HOME`, `JAVA_HOME`                                                                                                                                                                                 |

`dx doctor` reports every one of these and is the fastest way to check the setup.

## Verified

**Web** — driven end-to-end in a headless browser against both the dev server and the release
bundle served by Axum, with **no console errors** in either: list renders and filters (typing
`inn` narrows to *Inn Valley Ride*); tab switch shows the planned trip; client-side navigation
to the detail screen; stats, 10 OSM tiles, the track path, the uPlot canvas with both series
labelled, and two gallery thumbnails from `/media`; a cold deep link to `/app/trips/1` renders
the same.

**Android** — the APK builds, is debug-signed, carries `lib/arm64-v8a/libmain.so`, bundles all
four vendored Leaflet/uPlot assets, and declares INTERNET plus `usesCleartextTraffic`.

**Confirmed on a real phone (2026-08-26)**, sideloaded and pointed at a laptop on the LAN:
the trip list renders, filtering works, the detail screen loads, **the map works including
pinch-zoom**, in-app back navigation works, and full-size photos open. Two things did not
work — the hardware back button and the photo thumbnails — both judged minor and left
unfixed; see below.

That settles the question the whole spike existed for: **a Dioxus Android app against this
API is feasible, from the same source as the web build.**

## Measurements

|                                     | Web                                                                       | Android                                              |
|-------------------------------------|---------------------------------------------------------------------------|------------------------------------------------------|
| Cold build                          | 98 s debug · 79 s release                                                 | 290 s debug · 187 s release                          |
| Incremental rebuild, non-RSX change | 9.4 s                                                                     | not measured                                         |
| RSX-only text change                | hot-reloaded in ~1 s, no rebuild                                          | not measured                                         |
| Artifact                            | 2.3 MB wasm raw / **633 KB gzipped** (+ 68 KB gzipped of vendored JS/CSS) | **17 MB APK** (29.5 MB arm64 `.so` before packaging) |

8-core machine, Rust 1.96.1. The wasm grew from 523 KB to 633 KB gzipped when the spike went
multi-platform — almost entirely `reqwest` replacing `gloo-net`. Rust hot-*patching*
(`dx serve --hot-patch`) was not tried; it is flagged experimental.

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
  pure logic (formatting, query building, URL normalization — 14 tests) natively with no wasm
  test runner. That was not a given.
- **The Android build worked on the first attempt** once the toolchain was in place — one
  `dx build`, no Gradle editing, no `cargo-ndk`, no manifest hand-writing. The only correction
  needed was the target triple.
- **Multi-platform cost almost nothing in component code.** `list.rs`, `detail.rs`,
  `filters.rs` and `format.rs` — the actual UI — needed *no* platform-specific code at all.
  Everything that changed was infrastructure, listed below.

### The correction that reframes the whole spike

The first version of these notes claimed a native mobile target has no Leaflet and no uPlot,
and that the map and chart therefore would not port. **That was wrong.** It is true of
`dioxus-native` (the Blitz renderer), but Dioxus's `mobile` feature resolves to
`dioxus-desktop` — wry + tao — so **an Android Dioxus app is an Android System WebView**. The
Rust runs natively and drives the webview; HTML, CSS, Leaflet and uPlot all work there
unchanged. That is why the same `interop.rs` serves both platforms.

The genuine portability limits are narrower, and each one cost real work:

| What broke off the web       | Why                                                                                                 | Fix                                       |
|------------------------------|-----------------------------------------------------------------------------------------------------|-------------------------------------------|
| Typed `wasm-bindgen` interop | Only exists on `wasm32`; on Android the Rust is native, so there is no wasm boundary to bind across | Rewrote uPlot to `document::eval`         |
| `gloo-net`                   | Browser-only                                                                                        | `reqwest` (fetch on wasm, hyper natively) |
| Relative `/api/…` URLs       | The webview page is served from the app's own origin                                                | Runtime base-URL setting                  |
| Server-relative photo URLs   | Same                                                                                                | Joined onto the base URL                  |
| `/static/vendor/…` assets    | No server to proxy them from                                                                        | `asset!` bundling into the APK            |

### The interop reversal

The first pass drove Leaflet through `document::eval` and uPlot through
`#[wasm_bindgen] extern "C"` bindings, deliberately, to compare them. The typed style won on
readability and compile-time checking, and the notes recommended it.

**Android inverts that recommendation.** `wasm-bindgen` externs are a wasm-only mechanism; on
a webview platform they cannot exist. `eval` is the only interop that works on both, so a
multi-platform Dioxus app pays for portability in untyped, unchecked JS strings — for *every*
JS library it touches, forever. What that costs in practice: no compile-time checking, no
IDE support inside the script, and errors that surface only in a webview console you cannot
easily read on a phone.

Two mitigations are already in the code and worth keeping: payloads go over `dioxus.recv()`
rather than being spliced into the script text (the same injection concern `html_escape`
handles server-side), and a `whenReady` guard waits for the library's global rather than
assuming script load order — the injected `document::Script` tags load asynchronously, so
racing them would produce a map that works on a slow network and fails on a fast one.

### Friction

- **A JS library owns its DOM subtree.** The map and chart containers must be elements Dioxus
  never re-renders with children, and both draws need a guard against running twice. Inherent
  to VDOM-plus-JS, not a Dioxus flaw, but the thing most likely to bite later.
- **`#[component]` requires `PartialEq` on every prop type**, which meant adding `PartialEq`
  derives to `TripSummary` and `TripDetail` in the shared crate. Component props constrain
  shared server types.
- **`asset!` rejects anything outside the crate, symlinks included.** The vendored Leaflet and
  uPlot files are now duplicated in `crates/ui-dioxus/assets/` (220 KB) alongside the copy in
  `public/vendor/` that the PoC UI uses. Small, but it is a second copy of a vendored library
  that can drift.
- **`reqwest` requires absolute URLs even on wasm**, where a fetch wrapper would happily take
  `/api/trips`. Sharing one HTTP client across platforms means the web build must resolve its
  own origin explicitly at startup.
- **Build output is not small.** 633 KB gzipped for two screens, and a 17 MB APK, both
  substantially framework floor rather than app code.

### What CORS turned out not to be

The plan assumed the server would need a CORS layer for the phone. It does not: the API calls
are made by `reqwest` inside the *native* Rust process (confirmed — `hyper` and `rustls` are
compiled into `libmain.so`), and CORS is a browser-enforced policy that never applies there.
The webview only loads the app shell, bundled assets, and `<img>` tags, none of which are
CORS-restricted. **No server change was needed**, which is the better outcome: a blanket
allow-origin layer on a server with no auth yet (US-19) would have been a real exposure.

`usesCleartextTraffic` *is* needed, though — not for the API (native sockets are unaffected)
but for the webview loading photo thumbnails over plain `http://` on the LAN. A deployed HTTPS
instance (ADR-0023) would not need it.

### Framework-independent findings (they apply to Leptos too)

- The **types crate split works and is cheap** — the server needed one changed line.
- **Response types are not shareable.** `PhotoResponse` (`Photo` + per-request `url` /
  `thumbnail_url`, per ADR-0015) lives in `http.rs`, so the client mirrors it by hand in
  `api.rs`. That is the one duplicated shape in the crate, and it will multiply as the UI
  grows. Before committing to a Rust UI, decide where response types live: moved into the
  types crate, or generated. A real ADR-0015 question, not a spike detail.
- **The `/app` + index-fallback shape works** and keeps the "binary + adjacent `public/`"
  deployable unit of ADR-0016 intact.
- **A mobile client needs a configurable server address** — and, eventually, credentials
  (US-19). The settings screen here is the minimum; a real app also needs to handle the
  address changing and the server being unreachable.

## Known gaps found on the device

Neither was fixed — the owner judged both minor and not worth spending spike time on. Both
have a plausible cause and a known direction, recorded here so a future implementation does
not rediscover them.

**Photo thumbnails do not load, while full-size photos do.** That asymmetry is the clue: the
thumbnail is an `<img>` rendered *inside* the webview, whereas the full photo is an `<a href>`
that leaves the app and opens in the external browser. wry serves the app from
`https://dioxus.localhost` (the string is in `libmain.so`), so an `<img>` pointing at
`http://<lan-ip>:3000/media/…` is **mixed content**, which Android WebView blocks by default
regardless of `usesCleartextTraffic` — that flag and mixed-content policy are separate
controls. OSM tiles are unaffected because they are already https. Likely fixes, in order of
preference: serve the archive over HTTPS (which the deployed instance of ADR-0023 does
anyway, making this disappear on its own), or set `MIXED_CONTENT_ALWAYS_ALLOW` on the webview.
**Not verified** — the diagnosis fits every observed symptom but was not tested.

**The hardware back button does not navigate back.** Dioxus's router tracks its own history;
nothing wires Android's `OnBackPressedDispatcher` (present in the APK via androidx, unused) to
`router.go_back()`. A real app must do this — an Android app that ignores the back button
feels broken — so it belongs on the cost side of the ledger, not the "free" side.

## What this still does not tell us

- Cold-start time, memory use, and map performance under real use were not measured — only
  that they were acceptable enough that the owner did not remark on them.
- Nothing about styling, accessibility, touch ergonomics, offline behaviour, or app-store
  packaging (a real release build needs a keystore; this APK is debug-signed).
- **iOS remains completely untested** and cannot be tested here — it needs macOS and Xcode.
  If iOS ever matters, that is an untouched risk, not a small one.
- No component-level test story was evaluated; only pure logic is covered by tests.

## For a fair Leptos comparison

Build the same screens against the same API and record the same numbers: cold build,
incremental rebuild, hot-reload behaviour, gzipped wasm, and the interop approach for Leaflet
and uPlot. The types crate and `GET /api/trips/:id` are already in place and framework-
agnostic, so a Leptos spike starts from the component work.

**Leptos has no equivalent multi-platform story** — it targets the web. The Android result
held up on the device, so that asymmetry is now a confirmed difference rather than a promise.
If mobile matters, the remaining question is whether Dioxus's web-side ergonomics are good
enough — not which framework is better on the web.

## Cleanup

The spike is additive and reversible. Dropping it means deleting `crates/ui-dioxus`, the
`/app` route + `paths::spa_dir`, and `tests/spike_spa_bundle.rs`. Keep `crates/types` and
`GET /api/trips/:id` either way — both are wanted by any UI and by US-16.
