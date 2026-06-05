# Trip Archive — Implementation Plan

> Companion docs: [`requirements.md`](./requirements.md) (user stories) ·
> [`architecture.md`](./architecture.md) (C4 diagrams) · [`adr/`](./adr/) (architecture decision records).

## Context

Stop relying on komoot for **organizing** trips (storing GPS tracks + photos, browsing them on a
map) while continuing to use komoot for **recording, route planning, and discovery**. This is a
personal, single-user, self-hosted archive — *not* a komoot clone. No route planning, no
recording, no social/discovery features. It is also explicitly a **learning opportunity for Rust
and geospatial data**.

Tracks are imported as **GPX** (exported from whatever app records them); Garmin Connect import is
a possible later extension. Photos come with EXIF and should be pinned on the map.

The repo is greenfield (only devcontainer + tooling config). The devcontainer has Rust and Node
installed.

### Locked-in decisions
- **Stack**: Rust full-stack with **Leptos** (SSR + hydration on **Axum**), built with `cargo-leptos`. ([ADR-0001](./adr/0001-rust-leptos-fullstack.md))
- **DB**: **SQLite** embedded via `sqlx`, local disk only. ([ADR-0002](./adr/0002-sqlite-local-disk.md))
- **Tracks**: GeoJSON blob in SQLite (`track` table). ([ADR-0003](./adr/0003-track-as-geojson-blob-in-sqlite.md))
- **Map**: **Leaflet** + **OpenStreetMap raster tiles** via wasm-bindgen interop. ([ADR-0005](./adr/0005-leaflet-osm-via-wasm-interop.md))
- **Photos v1**: EXIF GPS → map pins; time-match non-geotagged photos to the track; auto thumbnails. (No captions yet.)
- Single user, self-hosted; auth optional. ([ADR-0010](./adr/0010-single-user-optional-auth.md))

### Known future extensions (v1 must not preclude these)
- **Android client** — PWA first; possibly native later. v1 keeps the API **JSON-first** ([ADR-0008](./adr/0008-json-first-api.md)).
- **Data on a private ownCloud instance** — photo blobs move to ownCloud via a `BlobStore` backend ([ADR-0007](./adr/0007-blobstore-abstraction.md)); the SQLite DB stays local.

## Architecture overview

A single Leptos crate (features `ssr` + `hydrate`, `crate-type = ["cdylib","rlib"]`). All
heavy/server-only code (sqlx, gpx, image, exif) lives under `src/server/`, gated behind
`#[cfg(feature = "ssr")]`, with those deps declared `optional = true` and pulled in only by the
`ssr` feature — this keeps them out of the wasm bundle (the single most important build rule).

**Storage split**: SQLite holds all trip data and always lives on local disk. The `trip` table
holds lightweight metadata + summary stats; a separate **`track`** table (1:1 with `trip`) holds
the full geometry as a **GeoJSON blob** (LineString with per-coord elevation + a timestamp/distance
array in `properties`). Splitting the blob into its own table keeps the list query
(`SELECT … FROM trip`) cheap while the trip-detail endpoint reads the blob and serves it raw to the
browser, which feeds it to Leaflet and the elevation chart. **Photos** (originals + thumbnails) are
the only on-disk/external blobs.

Photo blob I/O goes through a **`BlobStore` trait** (`src/server/storage.rs`) — `put`/`get`/`url_for`
— with a `LocalDisk` impl for v1 (under the data dir, served via `tower-http` `ServeDir`). A future
`OwnCloudWebDav` impl swaps in without touching the import pipeline or UI.

**API is JSON-first** ([ADR-0008](./adr/0008-json-first-api.md)). Reads/writes are plain Axum JSON
handlers: `GET /api/trips` (with filter params — activity, date interval, distance, free-text `q`,
and `bbox` region), `GET /api/trips/:id`, `GET /api/trips/:id/track.geojson` (reads the blob from
the `track` table), `GET /api/trips/:id/gpx` (downloads the original GPX verbatim, US-21),
`POST /api/import`, `POST /api/trips/:id/photos` (add photos to an existing trip),
`PATCH /api/trips/:id` (edit name + activity type), `DELETE /api/trips/:id`. The Leptos UI
consumes these same endpoints.

**Import = a plain Axum route, not a Leptos server function.** A normal
`<form enctype="multipart/form-data" action="/api/import">` posts a GPX + N photos; the handler
uses `axum`'s `Multipart` to stream each photo to disk (never buffering all in RAM), runs the
CPU-heavy parsing on `tokio::spawn_blocking` (optionally `rayon` for parallel thumbnailing), writes
rows in one transaction, and redirects to the trip page. ([ADR-0004](./adr/0004-import-via-axum-multipart.md))

## Project layout

```
Cargo.toml            # ssr/hydrate features; optional server deps; [package.metadata.leptos]
rust-toolchain.toml   # pin stable + wasm32-unknown-unknown
migrations/            # 0001_init.sql, 0002_track_original_gpx.sql, …
.sqlx/                # committed offline query cache
public/
  js/leaflet_glue.js  # thin Leaflet ES-module wrapper
  js/elevation_glue.js# uPlot wrapper
  vendor/             # pinned leaflet.{js,css}, uplot.{js,css}
style/main.scss
data/                 # gitignored runtime dir (TRIP_ARCHIVE_DATA_DIR)
  trip-archive.db     # holds trip + track(GeoJSON + original GPX) + photo metadata
  photos/<trip_id>/{orig,thumb}/<photo_id>.<ext>
src/
  main.rs             # ssr: Axum bootstrap; hydrate entry
  lib.rs / app.rs     # App root + router (shared)
  components/         # trip_list (+ filter bar & region-select map), trip_detail (+ edit),
                      #   import_form (activity type + date-prefixed name), map, elevation, gallery
  server/             # ssr-only: db, import, gpx, photo, geojson, state, storage, api
                      #   storage.rs -> BlobStore trait + LocalDisk impl (ownCloud later)
                      #   api.rs      -> JSON handlers (trips list/detail/import)
  models.rs           # shared serde structs (also the JSON API contract)
  server_fns.rs       # #[server] fns (thin wrappers over the JSON API where convenient)
  bindings/leaflet.rs # wasm-bindgen externs -> leaflet_glue.js
```

Axum mounting (ssr):
```
Router::new()
  .route("/api/trips", get(list_trips))                       // filter/search query params
  .route("/api/trips/:id", get(get_trip).patch(edit_trip).delete(delete_trip))
  .route("/api/trips/:id/track.geojson", get(serve_track_geojson))
  .route("/api/trips/:id/photos", post(add_photos))           // US-2: add photos later
  .route("/api/import", post(server::import::handle_import))
  .nest_service("/photos", ServeDir::new(data/photos))
  .layer(RequestBodyLimitLayer::new(...))   // cap upload size
  .leptos_routes(&opts, routes, App)
  .fallback(leptos_axum::file_and_error_handler(...))
  .with_state(app_state)                     // SqlitePool + data_dir
```

## Data model (`migrations/0001_init.sql`)

```sql
trip(
  id INTEGER PRIMARY KEY, name TEXT NOT NULL,
  activity_type TEXT NOT NULL,        -- e.g. 'cycling' | 'hiking' | ... (US-11; editable US-15)
  start_time TEXT, end_time TEXT, duration_secs INTEGER,
  distance_m REAL NOT NULL, ascent_m REAL, descent_m REAL,
  min_lat REAL, min_lon REAL, max_lat REAL, max_lon REAL,
  created_at TEXT NOT NULL
);
-- 1:1 with trip; kept separate so list queries never load the heavy columns
track(
  trip_id INTEGER PRIMARY KEY REFERENCES trip(id) ON DELETE CASCADE,
  geojson TEXT NOT NULL,       -- LineString + elevation/distance arrays in properties
  gpx     BLOB NOT NULL        -- the original uploaded GPX, stored verbatim (US-21)
);
photo(
  id INTEGER PRIMARY KEY,
  trip_id INTEGER NOT NULL REFERENCES trip(id) ON DELETE CASCADE,
  orig_file TEXT NOT NULL, thumb_file TEXT NOT NULL,
  taken_at TEXT, lat REAL, lon REAL,
  location_source TEXT NOT NULL,   -- 'exif' | 'interpolated' | 'none'
  width INTEGER, height INTEGER, created_at TEXT NOT NULL
);
CREATE INDEX idx_photo_trip ON photo(trip_id);
-- filter/search support (US-13/US-14; see ADR-0011)
CREATE INDEX idx_trip_activity ON trip(activity_type);
CREATE INDEX idx_trip_start    ON trip(start_time);
CREATE INDEX idx_trip_distance ON trip(distance_m);
CREATE INDEX idx_trip_bbox     ON trip(min_lat, min_lon, max_lat, max_lon);
```
Connect-time pragmas: `foreign_keys = ON`, `journal_mode = WAL`, `busy_timeout`.

## Import pipeline (`src/server/import.rs`)

1. Stream multipart: GPX → small buffer; each photo → `data/photos/<trip_id>/orig/`.
2. `spawn_blocking`: parse GPX (`gpx`), compute stats — total distance (`geo` haversine),
   ascent/descent from `ele` deltas, duration from timestamps, bbox, start/end.
3. `spawn_blocking` (rayon per photo): read EXIF (`kamadak-exif`) GPS + `DateTimeOriginal`;
   generate thumbnail (`image`), honoring EXIF orientation.
4. Time-match photos lacking GPS: binary-search timestamped track points, interpolate lat/lon;
   mark `location_source` = `exif` / `interpolated` / `none`. ([ADR-0009](./adr/0009-utc-timestamp-normalization.md))
5. Build the GeoJSON (coords + elevation + distance/time arrays in `properties`); in one
   transaction `INSERT` the `trip` row (incl. the form's **activity type** (US-11) and **name** —
   the UI pre-fills a `YYYY-mm-dd` date prefix from the track start (US-12)), the `track` row
   (GeoJSON blob), and the `photo` rows.
6. Redirect to `/trips/{id}`.

The photo half of this pipeline (stream → EXIF → thumbnail → time-match → insert `photo` rows) is
factored into one function also called by **`POST /api/trips/:id/photos`** to add photos to an
existing trip (US-2), time-matching against that trip's already-stored track.

*Future Garmin import* plugs in here as an alternate ingestion source producing the same internal
`ParsedTrack` + `Vec<ParsedPhoto>` before step 5. Not built now.

## Leaflet ↔ WASM interop (the trickiest client piece)

- Vendor `leaflet.{js,css}` in `public/vendor/`; expose a tiny stable API from
  `public/js/leaflet_glue.js` (`initMap`, `addTrackGeoJSON`, `addPhotoMarkers`).
- Bind to it in `src/bindings/leaflet.rs` via `#[wasm_bindgen(module = "/public/js/leaflet_glue.js")]`.
- In `src/components/map.rs`: a `NodeRef::<html::Div>` container + an `Effect` that runs
  **client-side after mount** to init the map, then `spawn_local` to fetch the GeoJSON
  (from `/api/trips/{id}/track.geojson`) and photo markers (a `#[server]` fn returning
  `Vec<PhotoMarker>`, converted with `serde-wasm-bindgen`) and add them. SSR renders only the
  empty div — Leaflet must never run during SSR (avoids hydration / "map already initialized").

## Elevation chart

Wrap **uPlot** (~40KB) with the same glue-module + wasm-bindgen pattern. x = cumulative distance
(or time), y = elevation — both emitted into the track GeoJSON `properties` at import, so the
client gets everything from one fetch. Optional later: hover-sync the chart cursor to a marker on
the map. ([ADR-0006](./adr/0006-uplot-elevation-chart.md))

## Key crates

ssr: `leptos` + `leptos_axum`/`leptos_meta`/`leptos_router`, `axum` (multipart), `tokio`
(rt-multi-thread, fs, macros), `tower-http` (fs, limit), `sqlx` (sqlite, runtime-tokio, macros,
migrate), `gpx`, `geo`, `kamadak-exif`, `image`, `rayon`, `serde`/`serde_json`, `chrono` (or
`time`), `anyhow`/`thiserror`, `tracing`(+subscriber).
hydrate: `wasm-bindgen`, `web-sys`, `serde-wasm-bindgen`, `console_error_panic_hook` (dev).
tooling: `cargo-leptos`, `sqlx-cli`.

## Build order (milestones)

- **M-docs — Project docs (done).** This plan, [`requirements.md`](./requirements.md), and the
  [`adr/`](./adr/) records. Living docs; update an ADR's *Status* if a decision is revisited.
- **M0 — Skeleton boots.** `cargo-leptos` project compiles both targets; Axum serves a hydrating
  "hello" page; reads `TRIP_ARCHIVE_DATA_DIR`. Proves the toolchain (highest risk) first.
- **M1 — Thin vertical slice.** `trip` + `track` migration; `BlobStore` trait + `LocalDisk` impl
  (for photos later); `POST /api/import` (JSON-first) for *GPX only*; parse, stats, insert trip +
  track(GeoJSON blob) in one transaction, redirect; trip-detail page fetches
  `/api/trips/:id/track.geojson` and renders the polyline on Leaflet. *Shippable spine.*
- **M2 — Trip list + full stats + metadata.** `GET /api/trips` JSON handler + list page consuming
  it; activity type (US-11) and date-prefixed name (US-12) captured at import and shown;
  ascent/descent/duration display, import-form polish.
- **M3 — Photos.** Accept photo files at import; stream to disk, EXIF GPS + thumbnails, `photo`
  rows, `ServeDir`, map markers + popups, gallery component. Add `POST /api/trips/:id/photos` to
  attach photos to an existing trip (US-2).
- **M4 — Time-matching.** Interpolate non-geotagged photos; style interpolated markers differently.
- **M5 — Elevation chart.** Emit distance/elevation arrays; uPlot component; optional map hover-sync.
- **M6 — Edit & delete trip.** `PATCH /api/trips/:id` to edit name + activity type from the detail
  page (US-15); `DELETE /api/trips/:id` (cascade + blob cleanup via `BlobStore`).
- **M7 — Filter & search.** `GET /api/trips` filter params + filter bar: activity type, date
  interval, distance, free-text name (US-13); geographic-region filter by drawing a rectangle on a
  map, matched against trip bbox columns (US-14; see [ADR-0011](./adr/0011-filtering-search-geo-queries.md)).
- **M8 — Hardening.** Optional single-password basic-auth middleware, upload limits, error UX,
  backups note.

**Deferred (not built in v1, but unblocked by the above):**
- **ownCloud storage** — add an `OwnCloudWebDav` `BlobStore` impl (WebDAV via `reqwest_dav`/`rclone`
  mount); keep the SQLite DB local; serve photos by redirect/proxy; keep a local thumbnail cache.
- **Android** — add a PWA manifest + service worker (cheapest), or a native client against the
  existing JSON API. Garmin Connect import likewise plugs into the existing import pipeline.
- **Komoot sync** (US-20) — optionally push name/activity-type edits back to the owner's komoot
  account. No decision made yet (no ADR); depends on komoot API access.

## Risks / gotchas

- **Feature leakage into wasm** — keep all server deps `optional` + `ssr`-gated; pin the wasm target. First builds are slow.
- **WASM bundle size** — keep `image`/`gpx`/charting server-side only; deploy `--release` (wasm-opt).
- **EXIF GPS parsing** — coords are DMS rationals + N/S/E/W ref; convert to decimal and apply the
  ref sign or coordinates come out wrong. Handle missing GPS gracefully (→ time-match).
- **GPX vs EXIF timezones** — see [ADR-0009](./adr/0009-utc-timestamp-normalization.md). Most likely cause of mis-pinned photos.
- **Large photos** — stream to disk, cap body size, decode/resize on blocking threads with a bounded pool.
- **SQLite locks** — WAL + `busy_timeout` so an import transaction doesn't collide with a page read.
- **OSM tile policy** — keep attribution + `maxZoom: 19`; fine for single-user.
- **Coarse region filter** — bbox-overlap matching (US-14) can yield false positives (a trip whose
  bounding box overlaps the drawn region but whose track never enters it). Accepted for v1; refine
  later by point-in-rectangle testing the LineString for surviving candidates. See [ADR-0011](./adr/0011-filtering-search-geo-queries.md).

## Verification

Development is **test-first (TDD)** per [ADR-0012](./adr/0012-tdd-test-strategy.md): each user
story is covered by a behaviour test (referencing its US id), logic modules have unit tests, and
API handlers have integration tests against a temp SQLite DB + `LocalDisk` `BlobStore` + fixtures;
mocks are used only for externals (network, clock). The manual/end-to-end checks below confirm the
automated suite on top of `cargo test`:


- After **M0**: `cargo leptos watch` (or `build`) succeeds for both targets; the page loads and
  hydrates (no console hydration errors); server logs show the data dir resolved.
- After **M1** (key end-to-end check): start the app, open the import page, upload a real GPX file,
  confirm redirect to the trip page and the track polyline draws on the map fit to its bounds;
  verify a `trip` row exists and a matching `track` row holds a GeoJSON blob with sane
  coordinates/bbox (`sqlite3 data/trip-archive.db 'select id,name,distance_m from trip;
  select trip_id,length(geojson) from track;'`).
- After **M3/M4**: upload a GPX + a mix of geotagged and non-geotagged photos; confirm thumbnails
  generated under `data/photos/<id>/thumb/`, markers appear at correct locations, and
  `photo.location_source` is `exif` vs `interpolated` as expected; spot-check an interpolated photo
  lands plausibly along the track.
- After **M5**: elevation chart renders and matches the track's elevation range.
- Cross-check derived stats (distance, ascent, duration) for one known trip against komoot's numbers
  for the same track — they should be close (haversine vs komoot's method differ slightly).
- Run `cargo test` (+ `cargo clippy`, `cargo fmt --check`); the suite covers logic units (stats
  math, DMS→decimal EXIF conversion, time-match interpolation, bbox-overlap filtering, GeoJSON
  building) and per-requirement API integration tests. See [ADR-0012](./adr/0012-tdd-test-strategy.md).
