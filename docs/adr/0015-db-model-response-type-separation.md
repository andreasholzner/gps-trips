# ADR-0015 — Separate DB models from API response types

## Status

Accepted

## Context

[ADR-0008](./0008-json-first-api.md) said "the contract is centralized in `models.rs`". In
practice, the DB model and the API response shape are not the same thing:

- The DB model holds every column the repo needs, including internal details (`blob_key`)
  that must not be exposed to clients.
- The API response may include computed fields (`url`, future `thumbnail_url`) that do not
  exist in the database.

The first version of US-7's photo gallery worked around this by putting a `url: String` field
on the `Photo` DB model with `#[serde(skip_deserializing)]`, defaulting to `""` after a
`repo::list_photos` call. This created a hidden contract: every HTTP handler returning photos
had to remember to populate `url` before serialising — and the type system gave no indication
that the field might be empty.

## Decision

**DB records and HTTP response types are distinct structs.**

- Structs in `src/models/` are plain data records that mirror the database schema.
  They derive only what the repo layer needs (`Debug`, `Clone`); no `Serialize` or `Deserialize`
  unless there is a concrete reason at the DB boundary.
- HTTP response types (`*Response`) live in `src/server/http.rs` and derive `Serialize`.
  They are constructed explicitly at the handler, accepting the DB record plus any
  computed fields as arguments — so the compiler enforces that every required field is
  provided.

The immediate example is `PhotoResponse`, built from a `Photo` and a `url`:

```rust
impl PhotoResponse {
    fn from_photo(photo: Photo, url: String) -> Self { … }
}
```

Future additions (thumbnail URL from US-5, coordinates from US-3/US-4) extend
`PhotoResponse`, not `Photo`.

## Consequences

- There is more boilerplate: each new API field requires a `*Response` struct field and a
  line in the constructor. This is intentional — the compiler enforces completeness.
- ADR-0008's claim that the contract lives entirely in `models.rs` no longer holds for
  response-only fields. The response types in `http.rs` are now part of the API contract.
- The `models.rs` types remain stable, importable by any future layer (background jobs,
  CLI tools) without pulling in HTTP concerns.
