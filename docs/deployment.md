# Deployment — self-hosting Trip Archive (US-10)

Trip Archive is a single Axum binary plus a folder of vendored static assets (map/chart
JS+CSS, ADR-0005/0006). The SQLite DB and photo blobs live under one configurable data
directory. No external services (no separate DB server, no cloud dependency) are required —
see [ADR-0002](./adr/0002-sqlite-local-disk.md), [ADR-0014](./adr/0014-defer-deployment-topology.md).

## Build a release binary

```sh
cargo build --release
```

This produces `target/release/trip-archive` and `target/release/komoot_check`. Migrations are embedded into the
`trip-archive` binary at compile time (`sqlx::migrate!`), so they don't need to ship separately.

## What to copy to the target machine

Two artifacts, kept **side by side** in the same directory:

```
trip-archive/
├── trip-archive        # target/release/trip-archive
└── public/             # the public/ directory from the repo root
```

The binary resolves its static assets relative to *its own location*, not the current working
directory, so this pair can be copied anywhere and started from any directory
([ADR-0016](./adr/0016-assets-relative-to-executable.md)).

## Configuration (environment variables)

| Variable | Default | Purpose |
|----------|---------|---------|
| `TRIP_ARCHIVE_DATA_DIR` | `./data` | Where the SQLite DB and photo blobs are stored. Set this to a persistent, backed-up location. |
| `TRIP_ARCHIVE_ASSETS_DIR` | `public/` next to the binary | Override the static assets location (e.g. if packaging into `/usr/share/trip-archive` while the binary lives in `/usr/bin`). |
| `RUST_LOG` | `trip_archive=info` | Standard `tracing-subscriber` env filter. |
| `KOMOOT_EMAIL` | unset | Komoot account email (US-22/US-27, [ADR-0021](./adr/0021-reverse-engineered-komoot-client.md)). Optional — see below. |
| `KOMOOT_PASSWORD` | unset | Komoot account password. Optional — see below. |

### Komoot sync (optional)

`KOMOOT_EMAIL`/`KOMOOT_PASSWORD` are only needed for the Komoot integration (`/komoot/sync`, and
the `komoot_check` CLI binary). Leaving either unset does not stop the server
from starting — every other page and API works normally; `/komoot/sync` itself returns a `400`
explaining the sync isn't configured. Set both to enable it:

```sh
KOMOOT_EMAIL=you@example.com KOMOOT_PASSWORD='...' TRIP_ARCHIVE_DATA_DIR=/path/to/data ./trip-archive
```

## Running

```sh
TRIP_ARCHIVE_DATA_DIR=/path/to/persistent/data ./trip-archive
```

The server listens on `127.0.0.1:3000` (laptop-local, on demand — ADR-0014). Start it when
organizing trips, stop it afterwards; there is no daemon/service setup required.

## Auth

None yet. The instance is unauthenticated (fine on a private network/VPN or `localhost`-only
use); a shared-password middleware is planned separately (US-19, ADR-0010) before exposing it
more broadly.
