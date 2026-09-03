# ADR-0010 — Single-user; optional shared-password auth

## Status

Accepted — amended once:
- 2026-09-02 — session cookie; the gate resolves a principal
  ([Amendment](#amendment-2026-09-02--session-cookie-the-gate-resolves-a-principal)).

## Context

The application has exactly one user, the owner, on a self-hosted instance
([US-10](../requirements.md)). Multi-user accounts, roles, and sharing are out of scope. The
owner may still want to keep a self-hosted instance from being wide open
([US-19](../requirements.md)).

## Decision

Ship **no user accounts** in v1. Provide an **optional single shared password** enforced by a
`tower` middleware layer (e.g. HTTP basic auth) that can be enabled via configuration.

## Consequences

- Minimal auth surface and no user-management complexity.
- An instance can be left open (e.g. behind a private network/VPN) or password-gated, owner's choice.
- If multi-user support is ever required, this ADR is **superseded** by a new one introducing
  accounts and authorization.

## Amendment (2026-09-02) — session cookie; the gate resolves a principal

`e.g. HTTP basic auth` was a placeholder from when the only UI was server-rendered HTML in the same
binary on the owner's laptop. Since then [ADR-0023](./0023-managed-scale-to-zero-hosting.md) put the
instance on the public internet and made this auth blocking,
[ADR-0024](./0024-dioxus-ui-web-and-android.md) put two clients on the far side of the JSON API — a
WASM SPA on the instance's own origin, and an Android app whose WebView reaches it cross-origin from
`https://dioxus.localhost` — and the owner named a likely next step: read-only access for other
people to a few trips, GPX included (US-53).

The deciding constraint is in the code. Photos load as `<img src>` against `/media/*path`
(`crates/ui-dioxus/src/photos.rs`) and the GPX download is an `<a href>` to `/api/trips/:id/gpx`
(`crates/ui-dioxus/src/detail.rs`) — plain URL loads that cannot carry an `Authorization` header, so
the credential must be one the browser attaches by itself. Basic auth qualifies on the web, but has
no logout, no expiry and no styleable login, and holds no cached realm for the cross-origin media the
Android WebView fetches.

### Decision

One secret and no user accounts is unchanged. The mechanism under it is now:

- **A session cookie, established by a login endpoint.** The endpoint takes the shared password and
  sets an `HttpOnly`, `Secure`, `SameSite=Lax` cookie; a matching endpoint ends it. The session is a
  signed token carrying its own expiry, not a row in a sessions table. The signing key is derived
  from the password, so rotating the secret is what revokes sessions. Expiry is long and sliding —
  the phone is the primary client.
- **Constant-time comparison, rate-limited logins.** One secret on the public internet, and on a
  scale-to-zero machine each failed attempt is also a wake-up the owner pays for.
- **A gate that resolves a principal, not a boolean.** The middleware puts `Owner` or `Anonymous`
  into the request extensions; routes and handlers read it. Media paths are already trip-scoped
  (`/media/trips/:id/…`), so per-trip authorization stays reachable.
- **Deny-by-default routing.** A route is protected unless allowlisted; the allowlist is the login
  endpoint, the static SPA bundle under `/app` (code, not data, and it must load to render the login
  screen), and a health endpoint if the platform needs one. An unauthenticated API request gets a
  JSON 401, not a redirect; the SPA turns that into its login screen.
- **`Authorization: Bearer` accepted as a second form of the same token**, so US-16's native
  `reqwest` client need not reach into the WebView's cookie store. This does not close the Android
  gap — media there is a cross-origin load carrying no cookie. The likely answer, a scoped token on
  the media URL, belongs to US-16, where it can be tested on a device.

Sharing (US-53) is not designed here. The principal exists so it can be added as a `Share` variant:
an unguessable link, scoped to named trips, read-only, revocable, no account for the recipient.
People the archive *knows* remain the multi-user case that supersedes this ADR.

### Consequences

- The "optional" in this ADR's title and second consequence is history: ADR-0023 made the password
  blocking, and US-48 makes a missing or empty secret refuse to boot.
- Every test that spawns a server ([ADR-0012](./0012-tdd-test-strategy.md)) must configure a
  password. The harness sets a known one in a single place and gains a test for the refusal itself.
  Whether a local development run is exempt is US-19's call; an exemption reachable in production
  would defeat the story.
- Deny-by-default is a property only if asserted — one table-driven test over the router covers every
  route's unauthenticated response.
- `SameSite=Lax` covers CSRF without a token: the multipart import and photo `POST`s are what a
  cross-site form would target, and `Lax` withholds the cookie there.
- **Signing out ends a browser's session, not a token.** The `DELETE` endpoint clears the cookie
  and the SPA drops the copy it holds, so a browser is left carrying no credential at all — which
  is the whole of the web case. A token a client has stored survives until it expires: the session
  is a signature over its own expiry, so there is no row to strike off.
- **Rotating the password is therefore the only revocation, and a blunt one** — every device signs
  in again, the phone included. Accepted: the alternative is the sessions table this decision does
  without, and on a phone the case revocation exists for is a lost or stolen one, where no button
  on that phone could be pressed anyway. US-16 ships no sign-out for that reason; the control is
  web-only, where "leave this machine clean while I am still holding it" is a real situation.
- `architecture.md`'s HTTP Router component ("optional shared-password auth middleware [planned,
  US-19]") understates this; it is updated when US-19 lands.
