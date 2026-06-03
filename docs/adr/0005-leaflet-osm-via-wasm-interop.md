# ADR-0005 — Leaflet + OSM raster tiles via wasm-bindgen interop

## Status

Accepted

## Context

The app needs an interactive map to render a track polyline and photo markers. There is no
mature native-Rust web-mapping library, so the Rust/WASM client must integrate a JavaScript map
library. OpenStreetMap public raster tiles were chosen for zero-key simplicity (no API key, no
external account).

## Decision

Vendor **Leaflet** (`leaflet.js` / `leaflet.css`) under `public/vendor/`. Expose a small, stable
API from an ES-module glue file (`public/js/leaflet_glue.js`: `initMap`, `addTrackGeoJSON`,
`addPhotoMarkers`) and bind to it from Rust with
`#[wasm_bindgen(module = "/public/js/leaflet_glue.js")]`. Initialize the map in a Leptos
component using a `NodeRef` for the container plus an `Effect` that runs **client-side after
mount**; `spawn_local` fetches the track GeoJSON (`/api/trips/{id}/track.geojson`) and photo
markers and adds them. During SSR the component renders only an empty container `<div>`.

## Consequences

- Only a tiny JS surface to maintain; Leaflet's full API is hidden behind the glue module.
- Leaflet must **never** run during SSR, or hydration fails ("map container already initialized").
- OSM tile usage policy must be respected: keep attribution and cap `maxZoom: 19`. Fine for
  single-user scale.
- Tiles depend on OSM's public service; switching to self-hosted tiles later is a glue-file change.
