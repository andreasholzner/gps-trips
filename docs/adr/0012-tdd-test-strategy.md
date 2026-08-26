# ADR-0012 — Test-Driven Development; requirement-covering tests, mock only externals

## Status

Accepted — amended three times:
- 2026-07-24 — clock seam realised as value-passing, not an injectable trait
  ([Amendment](#amendment-2026-07-24--the-clock-seam-is-value-passing-not-a-trait)).
- 2026-08-26a — the view layer is testable and now needs tests
  ([Amendment](#amendment-2026-08-26a--the-view-layer-is-testable-and-now-needs-tests)).
- 2026-08-26b — Playwright for the browser layer, capped by rule
  ([Amendment](#amendment-2026-08-26b--playwright-for-the-browser-layer)).

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
   non-deterministic: the network (komoot, mocked behind the `KomootClient` trait from
   [ADR-0021](./0021-reverse-engineered-komoot-client.md); future ownCloud WebDAV, Garmin
   Connect) and the system **clock** (kept out of the tested logic so time-matching/date
   logic is deterministic — see the amendment for the mechanism). Internal collaborators are
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
   *(Superseded by the 2026-08-26a amendment: the framework changed and the view layer both
   can and must be tested.)*
4. **API handlers get integration tests** that drive the Axum routers in-process (e.g.
   `tower::ServiceExt::oneshot`, `#[tokio::test]`) against the real temp DB + `LocalDisk` + fixtures
   — covering import, add-photos, list+filters, edit, delete end to end.

Testing uses the built-in `cargo test` harness; CI runs `cargo test`, `cargo clippy`, and
`cargo fmt --check`. Coverage may be tracked with `cargo-llvm-cov` as a guide (a tool, not a target).
*(Amended 2026-08-26b: a browser-level layer runs outside `cargo test`, in a second CI job.)*

## Consequences

- Slower initial coding in exchange for a living, executable specification and a regression-proof
  refactor step — the point of TDD.
- Tests assert behaviour and survive refactors, because internal collaborators are real; only
  genuine externals are mocked, so the suite does not ossify around implementation detail.
- Requirement → test traceability complements the requirement → ADR traceability table.
- A small **fixtures** corpus (GPX tracks of varying size/timezone; geotagged and non-geotagged
  photos; an out-of-range-timestamp photo) must be maintained as the canonical test inputs.
- The network seam must be injectable from the start, which slightly shapes the API of the
  sync code (realised as the `KomootClient` trait, [ADR-0021](./0021-reverse-engineered-komoot-client.md)).
  The clock turned out to need no injectable seam — see the amendment.
- Per-test SQLite uses a temporary file (not shared `:memory:`) so WAL/connection semantics match
  production ([ADR-0002](./0002-sqlite-local-disk.md)).

## Amendment (2026-07-24) — the clock seam is value-passing, not a trait

The original decision anticipated an *injected* clock. In practice no clock trait was ever
needed: photo↔track time-matching and all date logic operate only on timestamps that arrive
as data (EXIF fields, GPX track times), and the few places that need "now" (row `created_at`,
export backup naming) read `OffsetDateTime::now_utc()` once at a boundary and pass it into
pure functions as a plain argument. Tests supply fixed values through the same parameters, so
determinism is preserved without any mock.

The principle of decision point 2 stands unchanged — non-determinism never lives inside the
tested logic; only the anticipated mechanism (trait injection) was replaced by the simpler
one (pass time as a value). `architecture.md`'s component notes describe the as-built pattern.

## Amendment (2026-08-26a) — the view layer is testable and now needs tests

[ADR-0024](./0024-dioxus-ui-web-and-android.md) replaced Leptos SSR + hydration with a Dioxus
UI that ships as a web SPA and an Android app. Two things in decision point 3 aged out with it:
the reference to Leptos, and — more importantly — the claim that *"thin view components need no
dedicated tests"*.

That claim was defensible when the view was server-rendered HTML built by plain functions, whose
output every acceptance test could assert on directly. It is not defensible for a
UI that fetches, filters, formats and routes on the client. **The behaviour of several user
stories now lives in the view layer**, and the tests that currently cover them assert on
server-rendered HTML that ADR-0024 marks as legacy.

### What changes

**Decision point 3 is amended:** view components are no longer exempt. Pure logic still belongs
in plain modules and is still unit-tested directly — that principle is unchanged and, in the
Dioxus crate, formatting and filter-query building already follow it. What is added is that the
components themselves are testable, on the host target, and should be tested where they carry
behaviour.

**Decision point 1 is unchanged in principle but at risk in practice.** Every requirement is
still covered by a test of its intended behaviour. But for the UI-facing stories that coverage
currently runs through the server-rendered pages, so retiring them would silently drop it. Hence
the migration rule below.

### How the view layer is tested

Three layers, all under `cargo test` on the host target — no browser, no wasm toolchain, no
device:

1. **Pure logic** — unchanged from decision point 3. Formatting, query-string building, URL
   normalization.
2. **Component render** — a component is rendered to an HTML string with `dioxus-ssr` and
   asserted on: shown values, formatting, and the links it generates. Components containing a
   `Link` must be wrapped in a router, which the harness does.
3. **Whole screen against a real server** — the screen is rendered against an in-process Axum
   instance on an ephemeral port, backed by a temporary SQLite database and `LocalDisk` blob
   store, and polled until its fetches resolve. **Nothing is mocked**, which is decision point 2
   applied unchanged: the server, the database and the blob store are real collaborators, and a
   UI test that stubbed the API would ossify around the very contract it should be checking.

The UI crate carries a shared harness for levels 2 and 3, and one worked exemplar of each.

### What still needs a browser or a device

This style renders; it does not interact. `dioxus-ssr` dispatches no events, so `oninput`
filtering, tab switching and button handlers are not exercised by it. `document::eval` does
nothing headless, so the Leaflet map and uPlot chart ([ADR-0025](./0025-js-widget-interop-via-eval.md))
draw into nothing.

Those are covered outside `cargo test`: a browser-driven check of the built bundle for
interaction and JS interop, and manual verification on a phone for Android. Neither is a
substitute for the three layers above. The browser layer is settled by the next amendment;
Android remains manual.

### Migration rule

**Before a server-rendered page is deleted, the acceptance assertions that ride on it move to a
screen-level test of the SPA.** Coverage transfers; it does not evaporate. Until a story's
assertions have been migrated, the page they depend on stays, whatever else is true of it.

## Amendment (2026-08-26b) — Playwright for the browser layer

The previous amendment named a gap: rendering a component to a string dispatches no events and
runs no JavaScript, so filtering-as-you-type, tab switching, and the map and chart that
[ADR-0025](./0025-js-widget-interop-via-eval.md) draws through `document::eval` are all invisible
to it. This amendment closes that gap with a tool, and — more importantly — bounds it with a rule.

### Decision

**Browser-level tests use Playwright**, run against the *shipped artifact*: the built web bundle
served by the real server, on a throwaway data directory seeded through the real import API.

This is the one place the project's tooling leaves Rust, and the choice was made against that
preference rather than in ignorance of it. Three things decided it:

- **The failure mode here is timing.** Every test in this layer races an async fetch, then a
  render, then a JS library drawing into a canvas. Playwright's auto-waiting addresses exactly
  that; the Rust WebDriver clients leave it to hand-rolled polling.
- **A Rust binding is not an option.** The `playwright` crate has been unmaintained since 2022.
  The genuine Rust alternative is a WebDriver client such as `thirtyfour` — actively maintained,
  and a reasonable choice — but it buys one language back at the cost of managing a driver
  process and re-implementing the waiting.
- **The "one `cargo test` command" advantage is smaller than it looks.** These tests need a built
  bundle, a running server and a browser however they are written, so they sit outside the
  default loop in any case.

The tooling is **test-only and never ships**: nothing in the deployed binary, the web bundle or
the APK depends on it, and the application remains all-Rust.

### The rule that keeps it small

**This layer covers only what the host-target layers structurally cannot** — real user events,
and JS-interop rendering. Anything that can be asserted by rendering a component to a string
belongs there instead, where it runs in milliseconds with no browser.

Without that rule a browser suite grows into the slow, flaky centre of gravity of the whole test
effort, and the fast layers stop being written. A test added here should be justified by naming
which of the two exemptions it needs.

### Consequences

- CI grows a second job: the default one stays `cargo test` / `clippy` / `fmt --check`; the
  browser job additionally builds the web bundle and installs a browser (~100 MB).
- The repository gains a Node package manifest and lockfile confined to the browser-test
  directory.
- Test failures in this layer are debuggable — traces are retained on failure — which matters
  disproportionately for a layer whose failures are usually timing, not logic.
- **Android is still not automated.** These tests drive a desktop browser; the Android app is
  verified by hand on a device. Automating it would need a device or emulator in CI and is
  deliberately not attempted.
- The decision is cheap to reverse. If the Node dependency becomes a burden, this layer is a
  handful of tests, and porting them to a Rust WebDriver client is bounded work.
