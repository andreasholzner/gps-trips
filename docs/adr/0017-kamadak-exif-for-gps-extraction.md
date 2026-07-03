# ADR-0017 — Use `kamadak-exif` for EXIF GPS extraction

## Status

Accepted

## Context

[US-3](../requirements.md) requires reading `GPSLatitude`/`GPSLongitude`/`GPSLatitudeRef`/
`GPSLongitudeRef` EXIF tags out of uploaded photo bytes to place geotagged photos on the map.
No EXIF-parsing dependency exists in the project yet. Per this project's convention of an ADR
per major library choice ([ADR-0001](./0001-rust-leptos-fullstack.md),
[ADR-0004](./0004-import-via-axum-multipart.md), [ADR-0005](./0005-leaflet-osm-via-wasm-interop.md),
[ADR-0006](./0006-uplot-elevation-chart.md), [ADR-0007](./0007-blobstore-abstraction.md)), a new
EXIF crate gets the same treatment.

Requirements: pure Rust (no C/FFI dependency, consistent with the existing `gpx`/`geo` choices
and keeping the build/deploy story simple per [ADR-0016](./0016-assets-relative-to-executable.md));
actively maintained; read-only (US-3 only reads GPS tags, never writes); able to read directly
from an in-memory `&[u8]` (photos arrive as `Vec<u8>` from the multipart upload,
[ADR-0004](./0004-import-via-axum-multipart.md), before being written to the `BlobStore`),
not requiring a filesystem path.

## Decision

Depend on **`kamadak-exif`** (crates.io name `kamadak-exif`, imported as `exif`),
`kamadak-exif = "0.6"`. It is pure Rust, has no unsafe FFI/C dependency, reads directly from an
in-memory byte slice via `Cursor`, and is the de facto standard pure-Rust EXIF reader.

GPS extraction is implemented behind a small dedicated module, `src/server/location.rs`, which
wraps `exif::Reader`/`exif::Tag`/`exif::In` and exposes only a narrow
`extract_gps(&[u8]) -> Option<GpsPosition>` surface to the rest of the codebase — so if a
different EXIF crate is ever substituted later, only `location.rs`'s internals change, not its
callers (`photos.rs`).

## Consequences

- One new pure-Rust dependency; no impact on the "binary + adjacent `public/` folder" deploy
  story ([ADR-0016](./0016-assets-relative-to-executable.md)) — no shared libraries to bundle.
- `location.rs`'s narrow public surface isolates the rest of the codebase from `kamadak-exif`'s
  API, so a future crate swap (e.g. write support, broader container formats) is localized.
- Any format `kamadak-exif` doesn't understand, or any photo without EXIF/GPS, simply yields
  `None` from `extract_gps` — the desired best-effort behavior with no extra format-detection
  logic needed at the call site.
- Meaningful unit tests of `extract_gps` need real (if minimal, hand-built) JPEG/EXIF byte
  structures rather than a mock of the crate's internals — consistent with
  [ADR-0012](./0012-tdd-test-strategy.md)'s instruction to test EXIF parsing against real
  fixture files.
