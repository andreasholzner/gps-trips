# ADR-0023 — Managed scale-to-zero hosting; mobile access via the web UI

## Status

Accepted — supersedes the deferral in [ADR-0014](./0014-defer-deployment-topology.md)

## Context

The PoC has validated the concept (import pipeline, Komoot sync, QMapShack export, DB model),
and the owner now wants access to trips from a mobile device. This is revisit trigger #1 of
ADR-0014. Constraints, updated as of this decision:

- **Online-only mobile use is acceptable** — no offline-in-the-field requirement, so the
  files-sync pivot (ownCloud as source of truth, per-device index, UUID keys) remains rejected.
- **Maintenance burden is the owner's primary concern.** An unmanaged VPS (OS patching,
  reboots, TLS, monitoring) is a permanent chore disproportionate to a single-user hobby app.
- The laptop is not always on, so laptop-plus-VPN is not viable.
- The only existing infrastructure is a hosted ownCloud (WebDAV) — storage, no compute.

## Decision

Deploy the existing single binary to a **managed scale-to-zero platform** — initially
**Fly.io**: a from-scratch container image around the static musl binary, the SQLite DB and
blobs on a **persistent volume** ([ADR-0002](./0002-sqlite-local-disk.md) unchanged), TLS and
ingress provided by the platform, and the machine auto-stopping between uses. The
run-on-demand usage pattern of ADR-0014 is preserved, just automated (wake on request,
~1 s cold start).

Prerequisites before exposure: the **shared-password auth** of
[ADR-0010](./0010-single-user-optional-auth.md) (US-19) becomes **required**, and the
instance is HTTPS-only.

Backup remains in the owner's existing **borg** workflow (external disk + remote cloud
storage). To support it, the app will provide a **consistent backup export** of the database
and blob store — a SQLite snapshot (backup API / `VACUUM INTO`) plus the blobs, retrievable
from the deployed instance (endpoint or CLI) so the laptop-side borg job can archive it like
any other directory. This is a follow-up feature (US-40); platform volume snapshots cover
the interim.

Mobile access is the **responsive web UI / PWA** over the existing JSON API
([ADR-0008](./0008-json-first-api.md)); a native app remains an option only if its effort
stays modest, unchanged from ADR-0014.

## Consequences

- Trips become reachable from any device; expected cost ≈ €1–2/month.
- Residual maintenance: redeploys when the app changes, an occasional base-image rebuild,
  and periodically confirming backups exist. No OS administration, no monitoring stack.
- Trip data (location history) now lives with a third-party platform — accepted; it is the
  same trust class as the existing hosted ownCloud, and the instance is auth-gated + HTTPS.
- The platform choice is deliberately low-commitment: a plain container + volume ports to
  any equivalent platform.
- US-19 (auth) is promoted from optional to a blocking prerequisite for going live.
- Integer surrogate keys ([ADR-0013](./0013-integer-surrogate-keys.md)) stay — the sync
  pivot that would have forced UUIDs is rejected under current constraints.
