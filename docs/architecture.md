# Trip Archive — Architecture (C4 model)

This document describes the architecture using the [C4 model](https://c4model.com/): **Level 1
System Context**, **Level 2 Containers**, and **Level 3 Components**. Level 4 (code) is
intentionally omitted — the source is the code-level truth.

Diagrams are written in Mermaid C4 syntax (renders on GitHub). Companion docs:
[`requirements.md`](./requirements.md) · [`adr/`](./adr/) · [`initial_plan.md`](./initial_plan.md) (frozen).

Legend: solid = v1; elements/relationships marked **[planned]** are future extensions
(US-16–US-20) that the architecture must not preclude, not part of v1.

---

## Level 1 — System Context

Who uses the system and what it talks to.

```mermaid
C4Context
    title System Context — Trip Archive

    Person(owner, "Owner", "The single user. Records and plans trips in komoot, then archives and browses them here.")

    System(tripArchive, "Trip Archive", "Self-hosted web app to organize trips: GPS tracks + photos on a map, with stats, search and filtering.")

    System_Ext(komoot, "komoot", "External SaaS used for recording, route planning and discovery. Source of exported GPX files. [planned] target for name/activity sync.")
    System_Ext(osm, "OpenStreetMap tile servers", "Public raster map tiles.")
    System_Ext(owncloud, "ownCloud [planned]", "Owner's private file storage for photo blobs.")
    System_Ext(garmin, "Garmin Connect [planned]", "Alternate source of recorded activities.")

    Rel(owner, komoot, "Records, plans, discovers; exports GPX")
    Rel(owner, tripArchive, "Imports GPX + photos, browses/searches/edits trips", "HTTPS / web browser")
    Rel(tripArchive, osm, "Fetches map tiles", "HTTPS")
    Rel(tripArchive, owncloud, "Stores/serves photo blobs [planned]", "WebDAV")
    Rel(tripArchive, garmin, "Imports activities [planned]", "HTTPS API")
    Rel(tripArchive, komoot, "Syncs name/activity edits [planned]", "HTTPS API")

    UpdateLayoutConfig($c4ShapeInRow="2", $c4BoundaryInRow="1")
```

**Notes**
- The owner keeps using komoot for recording/planning/discovery; Trip Archive only *organizes*
  exported trips. GPX export → import is the integration in v1.
- Map tiles come from OpenStreetMap directly (no API key) — see
  [ADR-0005](./adr/0005-leaflet-osm-via-wasm-interop.md).

---

## Level 2 — Containers

The deployable/runtime pieces inside Trip Archive and how they communicate. Everything runs on the
owner's self-hosted machine.

```mermaid
C4Container
    title Container diagram — Trip Archive

    Person(owner, "Owner", "Single user, via a web browser")

    System_Boundary(ta, "Trip Archive (self-hosted)") {
        Container(spa, "Web UI", "Rust → WASM (Leptos, hydrated) + vendored Leaflet & uPlot", "Renders trip list, detail map, elevation chart, gallery, import & filter UI. Runs in the browser.")
        Container(server, "Application Server", "Rust (Axum + Leptos SSR), single binary", "Serves SSR pages and a JSON API; handles GPX/photo import, stats, filtering, edit/delete.")
        ContainerDb(db, "Database", "SQLite (single local file)", "trip metadata + stats, track (GeoJSON blob), photo metadata. Always on local disk.")
        Container(blobs, "Photo Store", "Local filesystem via BlobStore trait", "Photo originals + generated thumbnails. Swappable backend.")
    }

    System_Ext(osm, "OpenStreetMap tiles", "Public raster tiles")
    System_Ext(komoot, "komoot", "GPX export source")
    System_Ext(owncloud, "ownCloud [planned]", "Photo blob backend")

    Rel(owner, spa, "Uses", "HTTPS")
    Rel(owner, komoot, "Exports GPX from")
    Rel(spa, server, "Loads pages; calls JSON API; uploads GPX+photos (multipart)", "HTTPS / JSON")
    Rel(server, db, "Reads/writes trip, track, photo rows", "sqlx (SQL)")
    Rel(server, blobs, "Stores originals/thumbnails; serves files", "file IO / ServeDir")
    Rel(spa, osm, "Fetches map tiles", "HTTPS")
    Rel(blobs, owncloud, "Backed by [planned]", "WebDAV")

    UpdateLayoutConfig($c4ShapeInRow="2", $c4BoundaryInRow="1")
```

**Notes**
- The **track GeoJSON lives in the DB** (a blob in the `track` table), not in the photo store —
  see [ADR-0003](./adr/0003-track-as-geojson-blob-in-sqlite.md). Only photos are external blobs
  ([ADR-0007](./adr/0007-blobstore-abstraction.md)).
- The API is JSON-first so a future Android/PWA client is additive
  ([ADR-0008](./adr/0008-json-first-api.md)).

---

## Level 3 — Components

### 3a. Application Server components

```mermaid
C4Component
    title Component diagram — Application Server

    Container(spa, "Web UI", "Leptos / WASM")
    ContainerDb(db, "Database", "SQLite")
    Container(blobs, "Photo Store", "filesystem")

    Container_Boundary(server, "Application Server") {
        Component(router, "HTTP Router", "Axum", "Routing, request-body limit, optional shared-password auth middleware.")
        Component(ssr, "Leptos SSR + Routes", "leptos_axum", "Server-side render of pages; serves hydration bundle.")
        Component(api, "Trip API Handlers", "Rust / Axum", "GET list (+filters), GET detail, PATCH edit, DELETE; serves track.geojson and the original GPX download.")
        Component(import, "Import Handler", "Rust / Axum multipart", "POST /api/import and /api/trips/:id/photos; streams uploads; orchestrates a transaction.")
        Component(gpx, "GPX Parser & Stats", "gpx + geo", "Parse track; compute distance, ascent/descent, duration, bbox, start/end.")
        Component(photo, "Photo Ingestion", "Rust (kamadak-exif, image, rayon)", "EXIF GPS/time, thumbnail, time-match to track.")
        Component(geojson, "GeoJSON Builder", "serde_json", "Build track LineString blob with elevation + distance/time arrays.")
        Component(repo, "Repositories", "sqlx", "trip/track/photo persistence; filter & bbox-overlap queries.")
        Component(store, "BlobStore (LocalDisk)", "Rust trait", "put/get/url_for for photo originals & thumbnails.")
        Component(clock, "Clock", "Rust trait", "Injectable UTC time for deterministic date/time logic.")
    }

    Rel(spa, router, "JSON API & page requests", "HTTPS")
    Rel(router, ssr, "Page render requests")
    Rel(router, api, "Trip CRUD + filter requests")
    Rel(router, import, "Multipart upload requests")

    Rel(import, gpx, "Parse + derive stats")
    Rel(import, photo, "Process photos")
    Rel(import, geojson, "Build track blob")
    Rel(import, repo, "Insert trip+track+photos in one transaction")
    Rel(photo, store, "Write originals & thumbnails")
    Rel(photo, clock, "Resolve timestamps (UTC)")
    Rel(api, repo, "Read/write; run filters")
    Rel(ssr, repo, "Read for SSR")
    Rel(repo, db, "SQL", "sqlx")
    Rel(store, blobs, "File IO")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

**Notes**
- `BlobStore` and `Clock` are traits — the seams that get mocked/replaced: ownCloud as a future
  `BlobStore` impl ([ADR-0007](./adr/0007-blobstore-abstraction.md)), and the clock as the only
  time source so time-matching is deterministic and testable
  ([ADR-0009](./adr/0009-utc-timestamp-normalization.md), [ADR-0012](./adr/0012-tdd-test-strategy.md)).
- The photo half of the pipeline is shared between full import and "add photos later"
  ([ADR-0004](./adr/0004-import-via-axum-multipart.md)).
- Filtering and the geographic-region (bbox) query live in the repositories, against `trip`
  columns only ([ADR-0011](./adr/0011-filtering-search-geo-queries.md)).

### 3b. Web UI components

```mermaid
C4Component
    title Component diagram — Web UI (Leptos client)

    Person(owner, "Owner")
    Container(server, "Application Server", "Axum + JSON API")
    System_Ext(osm, "OpenStreetMap tiles")

    Container_Boundary(spa, "Web UI") {
        Component(approuter, "App Router", "leptos_router", "Client-side routing between pages.")
        Component(list, "Trip List + Filter Bar", "Leptos", "Lists trips with stats; activity/date/distance/name filters; region-select map.")
        Component(detail, "Trip Detail", "Leptos", "Composes map, elevation, gallery; inline edit of name + activity type.")
        Component(importform, "Import Form", "Leptos", "GPX + photos upload, activity type, date-prefixed name.")
        Component(map, "Map", "Leptos + Leaflet binding", "Track polyline + photo markers via wasm-bindgen glue.")
        Component(elev, "Elevation Chart", "Leptos + uPlot binding", "Elevation vs distance/time via wasm-bindgen glue.")
        Component(gallery, "Photo Gallery", "Leptos", "Thumbnails; links markers ↔ photos.")
    }

    Rel(owner, approuter, "Navigates", "HTTPS")
    Rel(approuter, list, "Route")
    Rel(approuter, detail, "Route")
    Rel(approuter, importform, "Route")
    Rel(detail, map, "Embeds")
    Rel(detail, elev, "Embeds")
    Rel(detail, gallery, "Embeds")

    Rel(list, server, "GET /api/trips (+filters)", "JSON")
    Rel(detail, server, "GET detail, track.geojson; PATCH/DELETE", "JSON")
    Rel(importform, server, "POST import / add photos", "multipart")
    Rel(map, osm, "Fetch tiles", "HTTPS")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

**Notes**
- Map and chart are thin Leptos wrappers over vendored JS (Leaflet, uPlot) through wasm-bindgen
  glue; map/chart code runs client-side only ([ADR-0005](./adr/0005-leaflet-osm-via-wasm-interop.md),
  [ADR-0006](./adr/0006-uplot-elevation-chart.md)).
- Reusable logic (stats, EXIF decode, time-match, bbox) lives in plain Rust modules on the server
  side, keeping these view components thin and the logic unit-testable
  ([ADR-0001](./adr/0001-rust-leptos-fullstack.md), [ADR-0012](./adr/0012-tdd-test-strategy.md)).

---

## Diagram ↔ decision map

| C4 element | Backing decision |
|------------|------------------|
| OSM tiles, Map component | [ADR-0005](./adr/0005-leaflet-osm-via-wasm-interop.md) |
| Elevation Chart | [ADR-0006](./adr/0006-uplot-elevation-chart.md) |
| Database container; track blob | [ADR-0002](./adr/0002-sqlite-local-disk.md), [ADR-0003](./adr/0003-track-as-geojson-blob-in-sqlite.md) |
| Photo Store / BlobStore | [ADR-0007](./adr/0007-blobstore-abstraction.md) |
| Import Handler / Photo Ingestion | [ADR-0004](./adr/0004-import-via-axum-multipart.md) |
| Trip API Handlers (JSON) | [ADR-0008](./adr/0008-json-first-api.md) |
| Clock seam (UTC) | [ADR-0009](./adr/0009-utc-timestamp-normalization.md) |
| Auth middleware | [ADR-0010](./adr/0010-single-user-optional-auth.md) |
| Filter/region queries in Repositories | [ADR-0011](./adr/0011-filtering-search-geo-queries.md) |
| Trait seams as test mocks | [ADR-0012](./adr/0012-tdd-test-strategy.md) |
