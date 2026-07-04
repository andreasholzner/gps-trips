# ADR-0019 — Use `tzf-rs` + `time-tz` for timezone lookup

## Status

Accepted

## Context

US-4 needs to resolve a photo's EXIF `DateTimeOriginal` (a local wall-clock time with no embedded
UTC offset, unless `OffsetTimeOriginal` is also present) to UTC, so it can be compared against the
track's UTC timestamps for interpolation (ADR-0009). ADR-0009 calls for "a configured trip-local
UTC offset... a per-import (or configurable) input."

Two ways to resolve that offset were considered:

- **Longitude heuristic** (`round(start_lon / 15)`): no new dependency, a few lines of pure code.
  Rejected — wrong for any region whose political timezone doesn't match its longitude band (e.g.
  all of Western Europe), and blind to daylight saving time entirely, which would misplace photos
  taken on trips that cross a DST boundary.
- **Real timezone-boundary lookup**: accurate, and (combined with a timezone database) correctly
  DST-aware per photo, not just per trip. Chosen, at the cost of a real dependency.

Per this project's convention of an ADR per major library choice (ADR-0001, 0004–0007, 0017), the
chosen dependency gets one here.

## Decision

Depend on **`tzf-rs`** (`default-features = false, features = ["bundled"]`, to skip the crate's
own CLI-only `clap` dependency) to resolve a `(lon, lat)` coordinate to an IANA timezone name (e.g.
`"Europe/Oslo"`) offline, via an embedded, pre-compiled timezone-boundary dataset — no network or
OS timezone data needed at runtime, keeping the single-binary deploy story (ADR-0016) intact.

Depend on **`time-tz`** (default features, which include the bundled IANA tzdata) to resolve that
IANA name plus a wall-clock `time::PrimitiveDateTime` to a DST-correct `time::OffsetDateTime`
(`PrimitiveDateTimeExt::assume_timezone`). Chosen specifically because it's designed as a
companion to the `time` crate this project already depends on, rather than pulling in
`chrono`/`chrono-tz` as a second, redundant date-time ecosystem.

Both are isolated behind a single new module, `src/server/timezone.rs`, which exposes only a
narrow coordinate → timezone → UTC-offset surface to the rest of the codebase — the same
isolation principle ADR-0017 established for `kamadak-exif`.

## Consequences

- `tzf-rs`'s embedded dataset adds roughly 6.6 MB to the binary, and its build (via `tzf-dist`)
  pulls in a `prost`/`prost-build` (protobuf codegen) build-time dependency chain. Verified this
  builds cleanly with no external `protoc` needed. Accepted as the cost of accurate, offline,
  DST-aware timezone resolution for a self-hosted single-binary app.
- Resolution is now correctly DST-aware **per photo** (using each photo's own capture date), not
  just per trip — a trip spanning a DST transition places photos correctly on both sides, which a
  static per-trip UTC-offset-in-minutes design could not do.
- `timezone.rs`'s narrow surface isolates the rest of the codebase from both crates' APIs, so a
  future substitution is localized, matching ADR-0017's precedent for `kamadak-exif`.
- An ambiguous "fall-back" DST instant or a nonexistent "spring-forward" wall-clock time both
  resolve deterministically (first solution, or `None` respectively) rather than panicking or
  picking arbitrarily — documented behavior, not silent chance.
