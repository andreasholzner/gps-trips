# ADR-0016 — Resolve static assets relative to the executable, not the CWD

## Status

Accepted

## Context

[US-10](../requirements.md) requires "single deployable binary + static assets" with no
external services. The binary already reads its data directory from
`TRIP_ARCHIVE_DATA_DIR` ([ADR-0002](./0002-sqlite-local-disk.md)), so a copied binary works
from anywhere as far as the DB and photo blobs are concerned.

The vendored map/chart assets ([ADR-0005](./0005-leaflet-osm-via-wasm-interop.md)/
[ADR-0006](./0006-uplot-elevation-chart.md)) were served via
`ServeDir::new("public")` — a path resolved against the process's **current working
directory**. That only works when the binary is run from the crate root (`cargo run`, or a
manual `cd` into the checkout). Copying the built binary elsewhere — the whole point of
"self-host on my own machine" per [ADR-0014](./0014-defer-deployment-topology.md) — broke
static assets, since `public/` wouldn't be found relative to wherever the binary happened to
be launched from.

## Decision

Resolve the assets directory in this order, in a small pure function
(`server::paths::assets_dir`, unit-tested directly):

1. `TRIP_ARCHIVE_ASSETS_DIR`, if set — explicit override, mirroring `TRIP_ARCHIVE_DATA_DIR`.
2. `public/` **next to the running executable** (`std::env::current_exe()`'s parent), if that
   directory exists — the real deployment layout: copy the binary and `public/` into the same
   folder and run it from anywhere.
3. Otherwise fall back to a CWD-relative `public` — preserves the `cargo run` dev workflow,
   where the executable lives under `target/debug/` but `public/` sits at the repo root.

Assets stay a **sibling folder**, not embedded into the binary (e.g. via `rust-embed`): this
matches the acceptance criteria's wording ("binary **+** static assets") and avoids adding a
dependency and rebuild-on-asset-change friction for a single-owner deployment.

## Consequences

- The binary can be copied and started from any working directory once `public/` sits beside
  it, closing the last CWD-dependent gap in US-10.
- Two artifacts to deploy (binary + `public/`), not one — acceptable per the acceptance
  criteria; embedding remains an option later if that changes.
- `TRIP_ARCHIVE_ASSETS_DIR` gives an explicit escape hatch (e.g. packaging into
  `/usr/share/trip-archive` while the binary lives in `/usr/bin`).
