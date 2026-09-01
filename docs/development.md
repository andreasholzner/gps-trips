# Development — working on Trip Archive

How to build, run and test the code. For shipping a built binary to the machine that will
serve it, see [`deployment.md`](./deployment.md) instead.

## Toolchain

| Piece | Version | Needed for |
|-------|---------|------------|
| Rust | stable (`rust-toolchain.toml`) | everything |
| `dx` (Dioxus CLI) | **0.7.10** | building/serving the SPA |
| `wasm32-unknown-unknown` target | — | building the SPA (`rustup target add wasm32-unknown-unknown`) |
| Node.js | 20+ | the browser test layer only — nothing that ships |

The `dx` CLI and the `dioxus` crate are pinned together and **must match**
([ADR-0024](./adr/0024-dioxus-ui-web-and-android.md)); the crate is pinned to `=0.7.10` in
`crates/ui-dioxus/Cargo.toml`. Install the CLI with
`curl -fsSL https://dioxuslabs.com/install.sh | bash`, or take the prebuilt tarball from the
matching GitHub release. `dx doctor` reports what it can and cannot find.

Android is a build target of the same crate but needs a JDK, the Android SDK and the NDK; that
toolchain is listed in [`dioxus-spike.md`](./dioxus-spike.md) and is only required when actually
building the app (US-16).

## Layout

A Cargo workspace of three crates:

| Path | What it is |
|------|------------|
| `.` (`trip-archive`) | the Axum server, plus the `komoot_check`, `komoot_backfill` and `qmapshack_export` CLI binaries under `src/bin/` |
| `crates/types` (`trip-archive-types`) | the data models shared by the server and the UI. Compiles for the server, for wasm and for Android; the SQLite mappings sit behind an optional `sqlx` feature only the server enables |
| `crates/ui-dioxus` | the Dioxus SPA ([ADR-0024](./adr/0024-dioxus-ui-web-and-android.md)) |

`tests/` holds the server's integration tests and, under `tests/browser/`, the browser layer.

## Running it

### The server

```sh
TRIP_ARCHIVE_DATA_DIR=./data cargo run --bin trip-archive
```

Serves the API on `http://127.0.0.1:3000`, with `/` redirecting to the SPA at `/app/` (which
has to be built first — see below). The last proof-of-concept page (`/komoot/sync`) is served
from here too, until US-44 replaces it; `/import` now redirects to the SPA's own screen. See
[`deployment.md`](./deployment.md) for every environment variable.

### The SPA, while working on it

Two terminals — the API, and the SPA's own dev server with hot reload:

```sh
# Terminal 1 — the API
cargo run --bin trip-archive

# Terminal 2 — the SPA at http://127.0.0.1:8080/app/
cd crates/ui-dioxus && dx serve --platform web
```

`Dioxus.toml` proxies `/api` and `/media` from the dev server to the API on port 3000, so the
SPA talks to a real server rather than a mock. Editing RSX hot-reloads in about a second;
anything touching Rust logic triggers a rebuild.

### The SPA, as it is actually served

The deployed shape is the built bundle served by Axum itself at `/app`, from `public/app`:

```sh
rm -rf target/dx/ui-dioxus public/app
(cd crates/ui-dioxus && dx build --release --platform web)
cp -r target/dx/ui-dioxus/release/web/public public/app
# → http://127.0.0.1:3000/app/
```

Wipe `target/dx/ui-dioxus` first, as above: `dx` keeps previously built, content-hashed
artifacts there, and copying without clearing it leaves orphaned `.wasm`/`.js` files in
`public/app`. `public/app` is generated and git-ignored.

## Tests

The strategy — test-first, real collaborators, mock only true externals — is
[ADR-0012](./adr/0012-tdd-test-strategy.md); it also decides *which* layer a given test belongs
in. Briefly: anything assertable by rendering to a string belongs in `cargo test`, and the
browser layer covers only what that structurally cannot.

### The default loop

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

`cargo test --workspace` runs the server's unit and integration tests, the shared crate's, and
the SPA's host-target tests — pure logic, components rendered to HTML with `dioxus-ssr`, and
whole screens rendered against a real in-process server on a temporary database.

### The browser layer

**`cargo test` does not run these.** They drive a real browser against the built bundle, so
they need the bundle built and installed into `public/app` first (see above).

```sh
cd tests/browser
npm install                      # first time only
npx playwright install chromium  # first time only — downloads a browser (~100 MB)
npm test
```

Run them from `tests/browser`: Playwright resolves its config relative to the working
directory, and from anywhere else the run fails with a bare "No tests found". The config starts
its own server on a fresh temporary data directory — your own `./data` is never touched — and
refuses to start with an explanatory message if `public/app` is missing.

Traces are retained for failures; `npx playwright show-trace test-results/<test>/trace.zip`
replays one. `test-results/` and `playwright-report/` are git-ignored.

Android has no automated tests at all and is verified by hand on a device
([ADR-0012](./adr/0012-tdd-test-strategy.md), [ADR-0024](./adr/0024-dioxus-ui-web-and-android.md)).

## Before committing

Build succeeds, `cargo test --workspace` is green, `clippy` is clean, `cargo fmt --all --check`
passes — and if the change touches the SPA's markup or behaviour, the browser layer too.
