# Architecture Decision Records

These ADRs capture the significant architectural decisions for **Trip Archive** (a self-hosted
komoot organization replacement). Each record follows the Michael Nygard format:
**Status / Context / Decision / Consequences**.

| ADR                                                  | Title                                                                       | Status                        |
|------------------------------------------------------|-----------------------------------------------------------------------------|-------------------------------|
| [0001](./0001-rust-leptos-fullstack.md)              | Rust full-stack with Leptos (SSR + hydration on Axum)                       | Superseded by 0024            |
| [0002](./0002-sqlite-local-disk.md)                  | SQLite (sqlx), local disk only                                              | Accepted                      |
| [0003](./0003-track-as-geojson-blob-in-sqlite.md)    | Track geometry as a GeoJSON blob in SQLite                                  | Accepted                      |
| [0004](./0004-import-via-axum-multipart.md)          | Import via native Axum multipart handler                                    | Accepted (amended 2026-09-01) |
| [0005](./0005-leaflet-osm-via-wasm-interop.md)       | Leaflet + OSM raster tiles via wasm-bindgen interop                         | Superseded by 0025            |
| [0006](./0006-uplot-elevation-chart.md)              | uPlot for the elevation chart                                               | Superseded by 0025            |
| [0007](./0007-blobstore-abstraction.md)              | `BlobStore` storage abstraction (local now, ownCloud later)                 | Accepted                      |
| [0008](./0008-json-first-api.md)                     | JSON-first API                                                              | Accepted                      |
| [0009](./0009-utc-timestamp-normalization.md)        | Normalize timestamps to UTC; document EXIF-offset assumption                | Accepted                      |
| [0010](./0010-single-user-optional-auth.md)          | Single-user; optional shared-password auth                                  | Accepted (amended 2026-09-02) |
| [0011](./0011-filtering-search-geo-queries.md)       | Filtering, search & geographic queries on SQLite (no PostGIS)               | Accepted                      |
| [0012](./0012-tdd-test-strategy.md)                  | TDD; requirement-covering tests, mock only externals                        | Accepted (amended 2026-08-26) |
| [0013](./0013-integer-surrogate-keys.md)             | Integer surrogate primary keys (revisit only for offline-first replication) | Accepted                      |
| [0014](./0014-defer-deployment-topology.md)          | Defer deployment topology; local-first for now                              | Superseded by 0023            |
| [0015](./0015-db-model-response-type-separation.md)  | Separate DB models from API response types                                  | Accepted                      |
| [0016](./0016-assets-relative-to-executable.md)      | Resolve static assets relative to the executable, not the CWD               | Accepted                      |
| [0017](./0017-kamadak-exif-for-gps-extraction.md)    | Use `kamadak-exif` for EXIF GPS extraction                                  | Accepted                      |
| [0018](./0018-enums-for-closed-string-sets.md)       | Prefer Rust enums over closed sets of string values                         | Accepted                      |
| [0019](./0019-tzf-rs-time-tz-for-timezone-lookup.md) | Use `tzf-rs` + `time-tz` for timezone lookup                                | Accepted                      |
| [0020](./0020-image-crate-for-thumbnails.md)         | Use the `image` crate for thumbnail generation                              | Accepted                      |
| [0021](./0021-reverse-engineered-komoot-client.md)   | Reverse-engineered Komoot client for automated import & edit sync           | Accepted                      |
| [0022](./0022-qmapshack-export.md)                   | One-way QMapShack database export                                           | Proposed                      |
| [0023](./0023-managed-scale-to-zero-hosting.md)      | Managed scale-to-zero hosting; mobile access via the web UI                 | Accepted                      |
| [0024](./0024-dioxus-ui-web-and-android.md)          | Dioxus UI: a CSR SPA on the web and an Android app from one source          | Accepted                      |
| [0025](./0025-js-widget-interop-via-eval.md)         | Vendored JS widgets driven from Rust through `document::eval`               | Accepted                      |

## Conventions

- ADRs are immutable once **Accepted**; to change a decision, add a new ADR that **supersedes**
  the old one and update both `Status` lines (e.g. `Superseded by ADR-00NN` / `Supersedes ADR-00MM`).
- Status values: `Proposed` → `Accepted` → `Superseded` / `Deprecated`.
