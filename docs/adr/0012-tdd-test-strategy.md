# ADR-0012 — Test-Driven Development; requirement-covering tests, mock only externals

## Status

Accepted

## Context

This project is, among other things, a learning exercise in Rust and geospatial data, and it
encodes precise behaviour (distance/elevation math, EXIF GPS decoding, photo↔track time-matching,
bbox region filtering) where regressions are easy to introduce and hard to spot by eye. The
requirements are expressed as concrete user stories with acceptance criteria
([requirements.md](../requirements.md)), which makes them directly testable. We want confidence
that each requirement is met and stays met as the milestones layer features on top of one another.

A choice of testing *philosophy* is needed: mockist (London-school — mock all collaborators) vs.
classical (Detroit-school — use real collaborators, mock only what you cannot control). Over-mocking
internal collaborators couples tests to implementation detail and undermines refactoring, which is
exactly what TDD's refactor step depends on.

## Decision

**Develop test-first (TDD): red → green → refactor.** A failing test that expresses the intended
behaviour is written before (or alongside) the code that satisfies it, for every milestone in the
plan.

1. **Every requirement is covered by a test of its intended behaviour.** Each user story (US-N)
   has at least one test asserting its acceptance criteria; tests reference the US id so coverage
   is traceable. These are behaviour/acceptance tests, not implementation snapshots.
2. **Mocks are used only for external dependencies** — things outside this process or
   non-deterministic: the network (future ownCloud WebDAV, komoot, Garmin Connect) and the system
   **clock** (injected so time-matching/date logic is deterministic). Internal collaborators are
   exercised for real:
   - the **database** via a real temporary SQLite file with migrations applied (one per test);
   - the **`BlobStore`** via its `LocalDisk` impl pointed at a `tempdir` (the trait from
     [ADR-0007](./0007-blobstore-abstraction.md) is also the seam where the *external* ownCloud
     backend gets mocked later);
   - **GPX / EXIF / image** parsing run against real fixture files (sample tracks, geotagged and
     non-geotagged photos).
3. **Components with relevant logic have unit tests**, with mocks only where a true external
   dependency is involved. Pure logic — haversine distance, ascent/descent, duration, DMS→decimal
   EXIF conversion, photo time-match interpolation, bbox-overlap filtering, GeoJSON building,
   date-prefix name formatting — is unit-tested directly. To keep this logic testable without the
   WASM toolchain, it lives in plain Rust modules separate from Leptos view code (consistent with
   [ADR-0001](./0001-rust-leptos-fullstack.md)); thin view components need no dedicated tests.
4. **API handlers get integration tests** that drive the Axum routers in-process (e.g.
   `tower::ServiceExt::oneshot`, `#[tokio::test]`) against the real temp DB + `LocalDisk` + fixtures
   — covering import, add-photos, list+filters, edit, delete end to end.

Testing uses the built-in `cargo test` harness; CI runs `cargo test`, `cargo clippy`, and
`cargo fmt --check`. Coverage may be tracked with `cargo-llvm-cov` as a guide (a tool, not a target).

## Consequences

- Slower initial coding in exchange for a living, executable specification and a regression-proof
  refactor step — the point of TDD.
- Tests assert behaviour and survive refactors, because internal collaborators are real; only
  genuine externals are mocked, so the suite does not ossify around implementation detail.
- Requirement → test traceability complements the requirement → ADR traceability table.
- A small **fixtures** corpus (GPX tracks of varying size/timezone; geotagged and non-geotagged
  photos; an out-of-range-timestamp photo) must be maintained as the canonical test inputs.
- The clock and network seams must be injectable from the start, which slightly shapes the API of
  the import/time-match and (future) sync code.
- Per-test SQLite uses a temporary file (not shared `:memory:`) so WAL/connection semantics match
  production ([ADR-0002](./0002-sqlite-local-disk.md)).
