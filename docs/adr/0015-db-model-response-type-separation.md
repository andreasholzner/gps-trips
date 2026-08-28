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

## Amendment (2026-08-28) — response types belong with the shared models

[ADR-0024](./0024-dioxus-ui-web-and-android.md) put a WASM web SPA and an Android app on the far
side of the JSON API. The placement rule above — response types live in the server's HTTP layer —
was written when the only consumer was a server-rendered page inside the same binary. That layer
depends on the web framework and the database driver, so it compiles for neither client target, and
the clients mirror the shapes by hand instead. ADR-0024 flagged this as needing a decision: one
duplicated shape today, multiplying as the UI grows.

The affected set is small. Most of what the API returns is a stored record serialized directly,
because its shape and the wire shape genuinely coincide. Only a few shapes are API-only — a photo
carrying computed URLs alongside its stored fields, and the request/response pair of the Komoot
sync — and those are the ones being mirrored.

### What changes: placement, not principle

Records and response types remain distinct, constructed explicitly at the boundary, with the
compiler enforcing completeness. That is unchanged. What changes is where the response types live:
**with the shared data models, in the crate that both the server and its clients compile**, so one
definition serves all of them.

Two rules keep that crate from becoming the undifferentiated module this ADR was written to end:

- **A stored record never grows a response-only field.** The pattern that prompted this ADR — a
  computed field bolted onto a record and left empty until a handler remembers to fill it — stays
  forbidden wherever the type lives.
- **A response type carries no server dependency.** It receives computed values as plain data;
  whatever produces them — storage, configuration, the request itself — stays on the server side. A
  shape that cannot meet that condition stays in the HTTP layer.

A stored record may serve as its own response type where the shapes coincide and no internal field
is exposed. That is not an exception to the principle: the principle is about not faking computed
fields and not leaking internal ones, never about the number of structs.

### Consequences

- The hand-mirrored shapes disappear, and drift becomes a compile error rather than something
  [ADR-0012](./0012-tdd-test-strategy.md)'s screen-level tests catch after the fact — and only for
  the fields a screen happens to read.
- The shared crate now holds two categories of type. The boundary between them is what makes this
  work; if it blurs, the symptom will be a computed field reappearing on a stored record, and this
  amendment will have failed.
- [ADR-0008](./0008-json-first-api.md)'s claim that the contract is centralized is closer to true
  than this ADR left it: the contract is one crate again, though still two kinds of type within it.
- No handler is rewritten. The types move and their constructors move with them; the server-side
  calls that produce computed values stay where they are.
- The first story that needs this is the trip-detail screen, the first to touch photos; the Komoot
  sync screen is the second. Landing it before then avoids mirroring a shape by hand and unmirroring
  it later.
