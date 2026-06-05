# ADR-0014 — Defer deployment topology; local-first for now

## Status

Accepted

## Context

The app needs to be *usable* before its deployment shape is settled. The owner wants the
"no server to run" property and has a **hosted ownCloud (WebDAV)** available, but committing now to
a more complex topology — an always-on cloud server, or an ownCloud files-sync model with a native
app — feels premature. The stated wish is a **working laptop solution first**, to judge from real use
whether the project is worth investing further in.

Constraints established while discussing options:
- Single user; **no parallel multi-device access** is needed.
- **Always has connectivity** (no offline-in-the-field requirement).
- A native Android app is a goal **only if the effort stays modest**.

Two observations shaped the decision: a native app is the single largest effort on the table and is
*easier* against a server/JSON API than in a sync model; and "always online + no parallel access"
removes the main reason to adopt the ownCloud/files-sync model. Neither end-state needs to be chosen
to make progress, and the current code already runs fine on a laptop.

## Decision

**Defer the deployment-topology choice.** Run **local-first / on demand**: the app is a single binary
that serves `localhost` with data under `TRIP_ARCHIVE_DATA_DIR` ([ADR-0002](./0002-sqlite-local-disk.md));
the owner starts it when organizing trips and stops it afterwards. This is a usable laptop solution
today and commits to nothing about cloud, ownCloud, phone, or native.

Keep the existing **seams** that hold both futures open (they cost nothing extra):
- the **`BlobStore`** trait ([ADR-0007](./0007-blobstore-abstraction.md)) — `LocalDisk` now, ownCloud-WebDAV or S3 later;
- **pure, framework-free domain logic** (GPX parsing, stats, GeoJSON) that a future native app could
  reuse as a shared Rust core — keep it free of axum/sqlx ([ADR-0001](./0001-rust-leptos-fullstack.md));
- the **JSON-first API** ([ADR-0008](./0008-json-first-api.md)), which keeps web/PWA *and* native open;
- **integer surrogate keys** ([ADR-0013](./0013-integer-surrogate-keys.md)) — don't adopt UUIDs unless
  the files-sync pivot actually happens.

This supersedes the exploratory "scale-to-zero cloud server" recommendation raised during the
discussion: **no topology is committed yet.**

## Revisit triggers

- *"I want it on my phone, away from the laptop"* → an always-on host, most likely **scale-to-zero
  cloud + a PWA** (native app later against the same API). Largely additive.
- *"I want offline use + a native app + plain files in my ownCloud"* → the **files-sync pivot**:
  files-as-source-of-truth in ownCloud, a local index per device, UUID keys, and revisiting
  ADR-0002/0003/0013. A real architectural change, not a tweak.

## Consequences

- A working laptop tool immediately, with no hosting, cost, auth, or ownCloud work required now.
- Deferral forecloses almost nothing: the cloud path is additive; the sync path remains possible via a
  one-time export/migration of the locally accumulated trips (easy while it is a single DB).
- Honest limitation: "zero server" and "simple, decide-later" cannot both fully hold at once —
  run-on-demand of a local binary is the lightest middle ground, and the true "files-only, no process"
  model is exactly the complex option being postponed.
- Auth ([ADR-0010](./0010-single-user-optional-auth.md) / US-19) is **not** needed while local-only,
  but becomes a prerequisite the moment the app is exposed on a network.
