# Komoot API — protocol reference

This is a **living reference doc**, not an ADR: it records the wire-level details of Komoot's
unofficial API (endpoints, auth mechanics, request/response shapes) so implementers don't have to
re-derive them. The *decision* to depend on this unofficial API, and the architecture built around
it (`KomootClient` trait, `trip_komoot_link`, "Sync now", backfill), lives in
[ADR-0021](./adr/0021-reverse-engineered-komoot-client.md) and links back here for the details.

Unlike an ADR, this doc is expected to change whenever a new endpoint is reverse-engineered or
Komoot changes something — not just at decision points.

## Source / provenance

Komoot has no official API. The endpoint and auth details below were derived by reading the source
of the open-source Python project [`Tsadoq/kompy`](https://github.com/Tsadoq/kompy) (specifically
`kompy/komoot_connector.py`, `kompy/authentication.py`, `kompy/constants/urls.py`) as of
2026-07-10. Treat all of it as liable to break without notice.

## Base URL

`https://api.komoot.de`

## Auth

Two auth mechanisms are known to work against this API:

- **HTTP Basic Auth per request** (what `kompy` does, and what v1 `KomootClient` uses): every
  request — login, list, get, upload, change, delete — sends `Authorization: Basic` with the raw
  account email + password. There is no session/cookie state to manage.
- **Session cookie** (observed in Komoot's own web UI): login establishes a session cookie that is
  then reused for subsequent requests, without resending credentials. Not used in v1.

Basic Auth was chosen for v1 for simplicity (no session lifecycle to manage). To keep a later
switch to cookie-based sessions cheap, `KomootClient`'s implementation should route every request
through one internal "make an authenticated request" seam rather than attaching Basic Auth
credentials at each call site individually.

The login call (see below) does return a `username` and a `password`-named field that reads like a
session token — but `kompy` never actually uses that token on later calls, it just re-sends
Basic Auth each time. So today, "login" is really "resolve the username + validate credentials
once," not "establish a session."

## Endpoints

### Login / credential check

```
GET /v006/account/email/{email}/
Auth: Basic (email, password)
```

- `200` — JSON body includes `username` (needed to build the list-tours URL below) and a
  `password`-named field (unused for subsequent calls in practice).
- `403` — bad credentials.

### List tours

```
GET /v007/users/{username}/tours/
Auth: Basic
```

Query params (all optional): `limit`, `page`, `status`, `type`, `only_unlocked`, `center`,
`max_distance`, `sport_types`, `start_date`, `end_date`, `name`, `sort_direction`, `sort_field`.

Response is HAL-style JSON: tours live at `_embedded.tours`; pagination info at `page.totalPages`
/ `page.number`.

### Get tour (metadata / GPX / FIT)

```
GET /v007/tours/{tour_id}
GET /v007/tours/{tour_id}.gpx
GET /v007/tours/{tour_id}.fit
Auth: Basic
```

Optional query param `share_token` grants access to a specific tour regardless of visibility.

- No suffix — JSON tour object.
- `.gpx` / `.fit` suffix — raw file bytes in that format.
- `404` — invalid tour id.
- `500` — transient, more common for `.fit`; retry or fall back to another format.

### Upload tour

```
POST /v007/tours/?data_type={gpx|fit}
Auth: Basic
Headers: User-Agent
```

Query params: `sport`, `status`, `name`, `data_type`, `time_in_motion` (GPX only). Body: raw
GPX/FIT bytes.

- `201` — created; response JSON has the new `id`.
- `202` — duplicate; a tour with the same content already exists, response JSON has its `id`.

### Change tour (name / activity type / privacy)

```
PATCH /v007/tours/{tour_id}
Auth: Basic
Headers: Content-Type: application/json
Body: { "sport": "...", "name": "...", "status"?: "..." }
```

- `200` — success.

### Delete tour

```
DELETE /v007/tours/{tour_id}
Auth: Basic
```

- `200` — success.

## Known gaps

- **No photo-fetch endpoint.** `kompy` only exposes `vector_map_image` (a small map thumbnail),
  not user-uploaded trip photos. Needed for the bulk backfill and future ingestion (photos are
  part of the "must have" scope) — has to be reverse-engineered separately before that work can
  start.
- **Session-cookie auth flow** is not reverse-engineered/documented here; only Basic Auth is
  covered. If Komoot ever locks down Basic Auth on this API, the session-cookie flow is the
  fallback to investigate.
