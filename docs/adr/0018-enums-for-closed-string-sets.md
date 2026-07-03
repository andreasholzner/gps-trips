# ADR-0018 — Prefer Rust enums over closed sets of string values

## Status

Accepted

## Context

Two columns so far hold a small, fixed, application-defined set of string
values with no DB-level enforcement: `trip.activity_type`
([0001_init.sql](../../migrations/0001_init.sql)) and `photo.location_source`
([0004_photo_location.sql](../../migrations/0004_photo_location.sql), added by
US-3). Both are `TEXT NOT NULL`, no `CHECK` constraint, "validated only by the
fixed set of values application code ever writes" — and on the Rust side both
are modeled as plain `String`/`&str`, so the compiler enforces nothing: a typo
(`"exf"` instead of `"exif"`), an unhandled new variant in a future `match`, or
an invalid value read back from the database are all only caught at runtime,
if at all.

The alternative — a Rust `enum` deriving `sqlx::Type` to map directly to the
stored string — moves that validation to compile time for every in-process
use, at the cost of a small amount of boilerplate per type (the derive and its
attributes) that a bare `String` doesn't need.

## Decision

Prefer a Rust **enum** over a bare `String`/`&str` for any field whose valid
values are a small, fixed, application-defined set — whether that set is
enumerated in a doc comment (as `activity_type`/`location_source` are today) or
would otherwise be validated ad hoc at the point of use.

The **database column type remains `TEXT`** where applicable (SQLite has no
native enum type); the Rust enum derives **`sqlx::Type`**
(`#[sqlx(rename_all = "lowercase")]` or explicit `#[sqlx(rename = "...")]` per
variant) so sqlx binds/reads it directly at the DB boundary, rather than a
manually-invoked `FromStr`/`Display` pair at each call site — more explicit
about the mapping, and it lives in one place on the type. JSON wire format
(`PhotoResponse`, etc.) is unaffected — enums serialize to the same lowercase
strings (`"exif"`, `"none"`, ...) the API already returns.

Does not apply to open-ended or user-supplied text (`original_name`,
free-text notes, etc.) — only to fields drawn from a fixed set the application
itself defines and exhaustively matches on.

## Consequences

- Invalid values become unrepresentable in Rust: a typo in a string literal is
  a compile error instead of a silent `"none"`-by-accident bug; adding a new
  variant (e.g. US-4's `"interpolated"`) forces every existing `match` to be
  updated or the compiler flags it as non-exhaustive.
- Each enum needs a `#[derive(sqlx::Type)]` (plus `#[serde(rename_all =
  "lowercase")]` if serialized) — a fixed, one-time cost per type, not per call
  site.
- The database schema is unaffected (`TEXT` columns, no migration needed
  purely for this decision) — only the Rust-side representation changes.
