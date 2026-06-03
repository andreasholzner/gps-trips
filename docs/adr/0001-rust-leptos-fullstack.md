# ADR-0001 — Rust full-stack with Leptos (SSR + hydration on Axum)

## Status

Accepted

## Context

The owner wants an all-Rust personal project. The application needs both a server side
(GPX/EXIF parsing, image processing, database access) and an interactive client side (a map,
an elevation chart, a photo gallery). The development environment already has Rust installed,
with Rust-leaning editor tooling.

Options considered:
- **TypeScript / SvelteKit** — richest ecosystem for maps/GPX/EXIF, fastest to build.
- **Rust backend + JS frontend** — Rust API with a separate JS SPA.
- **Rust full-stack (Leptos / Dioxus)** — Rust on both ends via WASM.

## Decision

Build a **single Leptos crate** with `ssr` + `hydrate` Cargo features
(`crate-type = ["cdylib", "rlib"]`), served by **Axum**, built and orchestrated by
**`cargo-leptos`**. Server-only code lives under `src/server/`, gated behind
`#[cfg(feature = "ssr")]`, with server dependencies declared `optional = true` and pulled in
only by the `ssr` feature so they never compile into the WASM target.

## Consequences

- One language end-to-end; shared types between server and client.
- Heavier, slower builds; `cargo-leptos` build complexity is the highest early risk.
- No native-Rust web map exists, so map rendering requires JS interop — see [ADR-0005](./0005-leaflet-osm-via-wasm-interop.md).
- Strict feature discipline is required to keep server crates (sqlx, image, gpx) out of the
  WASM bundle.
