# Deployment — running Trip Archive (US-10, US-45…US-49)

Trip Archive is a single Axum binary plus a `public/` folder holding the SPA's built bundle.
The SQLite DB and photo blobs live under one configurable data directory; no external services
are required ([ADR-0002](./adr/0002-sqlite-local-disk.md)). It runs in two places: on a laptop,
started on demand (the sections below), and as the deployed archive on Fly.io
([ADR-0023](./adr/0023-managed-scale-to-zero-hosting.md); see
[Deployed on Fly.io](#deployed-on-flyio)).

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

| Variable                  | Default                      | Purpose                                                                                                                                                                                                                    |
|---------------------------|------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `TRIP_ARCHIVE_DATA_DIR`   | `./data`                     | Where the SQLite DB and photo blobs are stored. Set this to a persistent, backed-up location.                                                                                                                              |
| `TRIP_ARCHIVE_ASSETS_DIR` | `public/` next to the binary | Override the static assets location (e.g. if packaging into `/usr/share/trip-archive` while the binary lives in `/usr/bin`).                                                                                               |
| `TRIP_ARCHIVE_BIND_ADDR`  | `127.0.0.1:3000`             | The address to listen on, as `IP:port` (US-45). Loopback unless set, so a laptop run is never reachable from the network by accident; the container sets `0.0.0.0:3000`. Anything else than an `IP:port` refuses the boot. |
| `TRIP_ARCHIVE_PASSWORD`   | **none — required**          | The one shared password (US-19, [ADR-0010](./adr/0010-single-user-optional-auth.md)). Missing or empty and the server refuses to start — see below.                                                                        |
| `RUST_LOG`                | `trip_archive=info`          | Standard `tracing-subscriber` env filter.                                                                                                                                                                                  |
| `KOMOOT_EMAIL`            | unset                        | Komoot account email (US-22/US-27, [ADR-0021](./adr/0021-reverse-engineered-komoot-client.md)). Optional — see below.                                                                                                      |
| `KOMOOT_PASSWORD`         | unset                        | Komoot account password. Optional — see below.                                                                                                                                                                             |

### The shared password (required)

The archive has one secret and no user accounts. Set `TRIP_ARCHIVE_PASSWORD` and the server
gates every route behind it; leave it unset or empty and **the server refuses to start** rather
than serving trips to whoever asks. That is deliberate and has no development exemption: an
exemption reachable in production would defeat the story it exists for.

```sh
TRIP_ARCHIVE_PASSWORD='…' TRIP_ARCHIVE_DATA_DIR=/path/to/data ./trip-archive
```

Signing in sets a cookie that lasts 90 days and slides forward with use, so a phone stays signed
in. Changing the password ends every session that exists — the sessions are signed with a key
derived from it, so there is nothing else to revoke; every other device shows its login screen
again the next time it fetches anything. Five consecutive failed sign-ins stop logins being
answered at all for fifteen minutes, for the instance as a whole.

Signing out on the web clears that browser's session and leaves nothing behind. It is deliberately
a web-only control: signing out is for a device you are walking away from but still holding, and
the case where a phone's access should be withdrawn — a lost or stolen one — is exactly the case
where no button on that phone can be reached. **Rotating the password is the answer there**, and
it is the answer whether or not a button exists (US-16).

The cookie is `Secure`, so the archive is reachable over HTTPS or over loopback (which browsers
treat as a secure context) — not over plain HTTP from another machine. The deployed instance is
HTTPS-only anyway ([ADR-0023](./adr/0023-managed-scale-to-zero-hosting.md), US-49).

### Komoot sync (optional)

`KOMOOT_EMAIL`/`KOMOOT_PASSWORD` are only needed for the Komoot integration (the SPA's sync
screen at `/app/komoot/sync`, and the `komoot_check` CLI binary). Leaving either unset does not
stop the server from starting — every other screen and API works normally; the sync endpoints
themselves return a `400` explaining the sync isn't configured, which the screen shows as it is.
Set both to enable it:

```sh
KOMOOT_EMAIL=you@example.com KOMOOT_PASSWORD='...' TRIP_ARCHIVE_DATA_DIR=/path/to/data ./trip-archive
```

## Running

```sh
TRIP_ARCHIVE_PASSWORD='…' TRIP_ARCHIVE_DATA_DIR=/path/to/persistent/data ./trip-archive
```

The server listens on `127.0.0.1:3000` unless `TRIP_ARCHIVE_BIND_ADDR` says otherwise. Start it
when organizing trips, stop it afterwards; there is no daemon/service setup required.

## Deployed on Fly.io

The deployed archive is the same binary in a container ([ADR-0023](./adr/0023-managed-scale-to-zero-hosting.md)):

- **Image** (`Dockerfile`): the static musl `trip-archive` and `komoot_check` binaries and the
  SPA bundle beside them, on Alpine — which adds only a shell and the `sqlite3` CLI for looking
  at the volume. It is built on Fly's remote builder with a pinned Rust version, so the laptop
  needs `flyctl` and nothing else.
- **Machine** (`fly.toml`): one `shared-cpu-1x` machine with 1 GB in `arn` (Stockholm). It is
  stopped when idle and started by the next request, which then takes about a second.
- **Volume**: `/data`, holding the database and the photos (`TRIP_ARCHIVE_DATA_DIR=/data`). It
  starts at 1 GB and grows by itself up to a limit that is provisional until US-50.
- **Address**: `https://<app>.fly.dev` with Fly's certificate. Plain HTTP is answered with a
  redirect to HTTPS by Fly's edge and never reaches the app.
- **Health check**: a TCP check on port 3000. The server binds only after the password check and
  the migrations, so a release that cannot boot fails the deployment.

### Exactly one machine

SQLite on a volume allows one writer, and a volume belongs to one machine. A second machine
would get a volume of its own: two archives, drifting apart, behind one address. Fly creates
two machines by default, so `scripts/deploy.sh` always deploys with `--ha=false` and refuses to
run when the app has more than one machine. Consequences, accepted:

- a deployment stops the old machine before the new one starts — a few seconds of downtime;
- there is no failover: if the volume's host fails, the archive is down until it returns or a
  snapshot is restored onto a new volume.

### The app name

The app name is the hostname. It is kept unguessable and **out of this repository**, which is
public: `fly.toml` has no `app` line, and the deployment script takes the name from `FLY_APP`. Set
it in your shell before any command below:

```sh
export FLY_APP=<the app name>
```

### First-time setup

```sh
curl -L https://fly.io/install.sh | sh    # flyctl
fly auth login
fly apps create "$FLY_APP"
```

Then the secrets (US-48). `fly secrets import` reads `NAME=VALUE` lines from standard input,
so no value lands in the shell history — type the lines, then Ctrl-D:

```sh
fly secrets import --app "$FLY_APP" --stage
TRIP_ARCHIVE_PASSWORD=…
KOMOOT_EMAIL=…
KOMOOT_PASSWORD=…
```

The Komoot lines are optional, as on the laptop. The first deploy creates the machine and the
volume:

```sh
scripts/deploy.sh
```

### Deploying a change

```sh
scripts/deploy.sh
```

It refuses to run with uncommitted changes — `fly deploy` ships the directory as it is on disk,
and only a clean tree makes what runs a commit — and with more than one machine (above). The
migrations run on boot, so a new schema needs no manual step.

### After a deployment

Checked by hand; this is platform configuration no test reaches (US-49):

```sh
curl -sI "http://$FLY_APP.fly.dev/" | head -3             # 301 to https://
curl -sI "https://$FLY_APP.fly.dev/app/" | head -1         # 200: the SPA loads
curl -s  "https://$FLY_APP.fly.dev/api/trips"              # 401 JSON: nothing without a session
```

### Changing a secret

The same `fly secrets import --app "$FLY_APP"` (without `--stage`) restarts the machine with
the new value. For `TRIP_ARCHIVE_PASSWORD` that ends every session on every device — the only
revocation there is (US-19).

### Looking inside

```sh
fly ssh console --app "$FLY_APP"
sqlite3 /data/trip-archive.db
```

`fly ssh console` needs a running machine; any request wakes it.
