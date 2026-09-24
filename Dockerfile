# syntax=docker/dockerfile:1
#
# The deployed archive (US-45, ADR-0023): the static musl server binary and
# the SPA bundle beside it (ADR-0016), on Alpine for a shell to inspect the
# volume with. `komoot_backfill` rides along: it writes the volume, so it runs
# inside the instance (US-51, ADR-0021's 2026-09-19 amendment). Built by
# `scripts/deploy.sh` on Fly's remote builder, so the laptop needs none of the
# toolchain below.

# ── Build ────────────────────────────────────────────────────────────────────
# Debian, not Alpine: the prebuilt `dx` links against glibc. `musl-tools` is
# for the C inside the musl binary — the bundled SQLite and `ring`.
#
# The Rust version is pinned here and bumped deliberately; development and CI
# float on `stable`. RUSTUP_TOOLCHAIN is what makes the pin hold: it outranks
# `rust-toolchain.toml`, which would otherwise fetch whatever stable is today.
FROM rust:1.98.1-slim-trixie AS build
ENV RUSTUP_TOOLCHAIN=1.98.1
# Pinned together with the `dioxus` crate and must match it (ADR-0024).
ARG DX_VERSION=0.7.10

RUN apt-get update \
 && apt-get install -y --no-install-recommends musl-tools curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl wasm32-unknown-unknown

# The same checksum-verified release tarball CI installs.
RUN set -eu; \
    base="https://github.com/DioxusLabs/dioxus/releases/download/v${DX_VERSION}"; \
    asset="dx-x86_64-unknown-linux-gnu"; \
    cd /tmp; \
    curl -fsSLO "${base}/${asset}.tar.gz"; \
    curl -fsSLO "${base}/${asset}.sha256"; \
    sha256sum --check --ignore-missing "${asset}.sha256"; \
    tar xzf "${asset}.tar.gz" -C /usr/local/bin; \
    rm "${asset}".*; \
    dx --version

WORKDIR /src
COPY . .

# What the trip list shows as the version (US-68), from `scripts/deploy.sh`;
# the server and the SPA both read it at compile time.
ARG TRIP_ARCHIVE_VERSION=dev
ENV TRIP_ARCHIVE_VERSION=${TRIP_ARCHIVE_VERSION}

# `target/` is a cache mount, so the artifacts are copied out to /out within
# the same step. `target/dx` is wiped first, as in CI: dx keeps earlier
# content-hashed artifacts there, and copying would carry them along.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    set -eu; \
    rm -rf target/dx/ui-dioxus; \
    (cd crates/ui-dioxus && dx build --release --platform web); \
    cargo build --release --locked --target x86_64-unknown-linux-musl \
        --bin trip-archive --bin komoot_check --bin komoot_backfill \
        --bin photo_taken_at_backfill; \
    mkdir -p /out/public; \
    cp -r target/dx/ui-dioxus/release/web/public /out/public/app; \
    cp target/x86_64-unknown-linux-musl/release/trip-archive \
       target/x86_64-unknown-linux-musl/release/komoot_check \
       target/x86_64-unknown-linux-musl/release/komoot_backfill \
       target/x86_64-unknown-linux-musl/release/photo_taken_at_backfill /out/

# ── Run ──────────────────────────────────────────────────────────────────────
FROM alpine:3.24
# For looking at the volume over `fly ssh console`; the server needs nothing
# from the image.
RUN apk add --no-cache sqlite

COPY --from=build /out/ /app/
ENV PATH="/app:${PATH}" \
    TRIP_ARCHIVE_BIND_ADDR=0.0.0.0:3000 \
    TRIP_ARCHIVE_DATA_DIR=/data
EXPOSE 3000
CMD ["/app/trip-archive"]
