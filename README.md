# Trip Archive

A personal, single-user, self-hosted archive for organizing trips — GPS tracks plus the photos
that go with them — and browsing them on a map. It replaces **komoot's organization features
only**: importing tracks, attaching and placing photos, browsing trips with stats, and reliving
one on a map. Recording, route planning and anything social stay in komoot.

The driver is owning the data: one Rust binary, one SQLite file, one folder of photos, all on
hardware the owner controls.

- **Server:** Axum + SQLite (`sqlx`), a JSON API, and a set of CLI binaries for the Komoot sync
  and the QMapShack export.
- **UI:** a Dioxus SPA, built from one crate for the web and for Android — the whole UI. The
  server-rendered proof of concept it replaced screen by screen is gone; `/` redirects to it.

## Documentation

| Document | What's in it |
|----------|--------------|
| [`docs/requirements.md`](./docs/requirements.md) | The user stories with their acceptance criteria, what's done, and the order the remaining work happens in. **The place to start.** |
| [`docs/architecture.md`](./docs/architecture.md) | C4 diagrams: system context, containers, components. |
| [`docs/adr/`](./docs/adr/) | Architecture Decision Records — every significant decision, its context and its consequences. [`adr/README.md`](./docs/adr/README.md) indexes them. |
| [`docs/development.md`](./docs/development.md) | How to build, run and test the code. |
| [`docs/deployment.md`](./docs/deployment.md) | How to self-host a built binary. |

Narrower references: [`docs/komoot-api.md`](./docs/komoot-api.md) (the reverse-engineered Komoot
API), [`docs/qmapshack.md`](./docs/qmapshack.md) and
[`docs/qmapshack-format.md`](./docs/qmapshack-format.md) (the QMapShack export and its database
format), and two spike records — [`docs/dioxus-spike.md`](./docs/dioxus-spike.md) (the spike that
settled the UI framework, including the Android toolchain) and
[`docs/eval-two-way-spike.md`](./docs/eval-two-way-spike.md) (whether the JS-interop mechanism
could carry a map reporting back into Rust).
[`docs/initial_plan.md`](./docs/initial_plan.md) is a frozen historical snapshot — the living
documents above supersede it.

## Quick start

```sh
cargo build --release
TRIP_ARCHIVE_DATA_DIR=./data cargo run --bin trip-archive
# → http://127.0.0.1:3000
```

[`docs/development.md`](./docs/development.md) covers the SPA's build and the test layers;
[`docs/deployment.md`](./docs/deployment.md) covers configuration and self-hosting.
