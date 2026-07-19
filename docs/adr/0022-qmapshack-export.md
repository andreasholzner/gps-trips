# ADR-0022 — One-way QMapShack database export

## Status

Proposed

## Context

Per the new "QMapShack export" section of [`requirements.md`](../requirements.md) (US-36,
US-37): the owner wants to browse and compare their whole trip archive (hundreds of trips) in
the desktop tool QMapShack, and reuse existing track segments there when planning new routes.
QMapShack is a **side-tool only** — no round-trip back into the archive; a planned route is
still imported into Komoot the established way ([ADR-0021](./0021-reverse-engineered-komoot-client.md)).

Two export shapes were considered (recorded in full in [`docs/qmapshack.md`](../qmapshack.md),
the discussion/spike log this ADR formalizes):

- **Plain GPX export**, reusing the existing per-trip download (US-21) and filters (US-13).
  Rejected as insufficient at the owner's scale (~1000 trips): flat GPX files have no
  organizational concept, and importing hundreds of individual files into QMapShack is
  uncomfortable compared to a single database the owner can just open.
- **A QMapShack-native SQLite database export**, giving a single-import experience with
  QMapShack's own folder/category organization. Chosen, once the format itself proved
  tractable — see below.

QMapShack's database format is undocumented but was fully reverse-engineered by reading
QMapShack's C++ source and cross-checking against two real `.db` files, one of them the
owner's actual ~4600-item production database. Full format detail lives in
[`docs/qmapshack-format.md`](../qmapshack-format.md); summary relevant to this decision:

- The relational schema (folders/items/hierarchy) is plain SQL with a `versioninfo` compat gate
  (`DB_VERSION`), unchanged for **~10 years** (since 2016-07-19) across QMapShack's git history.
- The one proprietary piece, `items.data`, is a Qt (`QDataStream`, `Qt_5_2`, little-endian)
  binary blob — undocumented but not encrypted or obfuscated. Its version tag (`VER_TRK`)
  changed 6 times in the project's first ~5 years, then unchanged for **~6.5 years** (since
  2020-01-03), across releases up to the current `V_1.20.3`.
- A stratified random sample of 17 items from the owner's real `Touren.db` (tracks, waypoints,
  the one route) matched the reverse-engineered structure exactly, including cross-checking the
  blob's embedded `hash`/`who` fields against the `items.hash`/`items.last_user` SQL columns.

Given this track record, targeting the current format is judged an acceptable, bounded risk —
not a guarantee, but strong precedent that it won't need rework soon.

[ADR-0014](./0014-defer-deployment-topology.md) commits the app to on-demand/local-first, no
background daemon — this export must be an **explicit, owner-triggered run**, not an in-app
scheduler, the same constraint [ADR-0021](./0021-reverse-engineered-komoot-client.md) worked
under for Komoot sync.

## Decision

- **A new one-off CLI binary**, `src/bin/qmapshack_export.rs`, following the
  `komoot_check`/`komoot_backfill` precedent ([ADR-0021](./0021-reverse-engineered-komoot-client.md)):
  reuses the existing lib crate (repos/models) directly — no new HTTP API. Run manually by the
  owner, or from the owner's own OS-level scheduler (e.g. cron) for "regularly" — this stays
  outside the app process, consistent with ADR-0014.
- **Full-library export**: every trip in the archive is exported, unfiltered. (Filtered/partial
  export via the existing US-13 query filters was considered and set aside — the whole point is
  a single always-current QMapShack copy of the archive, not a filtered snapshot.)
- **One-way sync only**: trip-archive → QMapShack database. Nothing is ever read back. Each run
  reconciles the target database to the archive's current state:
  - A trip not yet present is **inserted** as a new `items` row (type = track).
  - A previously-exported trip is only written if it actually **changed**: for each trip matched
    by `keyqms`, the exporter compares the trip's current name and all fields feeding the folder
    mapping (currently name and activity_type — see below; automatically covers any field added
    to the mapping later) against the target database's existing state for that item (`items.name`
    and its current `folder2item` placement), and only issues an `items` **update** — full blob
    rewrite (`items.data`'s hash/last_change follow from that), not a patch — and/or a
    `folder2item` re-link when something actually differs. An unchanged trip is skipped entirely,
    so repeated runs don't churn history events or folder links for no reason.
  - A previously-exported trip that no longer exists (deleted per US-9) is **removed** by
    deleting its `folder2item` link(s), letting QMapShack's own `folder2item_delete` trigger
    move it to trash (`items.trash` timestamp) rather than issuing a hard `DELETE FROM items` —
    reuses QMapShack's existing trash mechanism instead of re-implementing one.
- **Item identity via a namespaced, deterministic `keyqms`** (e.g. `trip-archive:trip:<id>`),
  not QMapShack's own MD5-hash convention. `keyqms` is a **pre-existing QMapShack column**
  (`items.keyqms TEXT NOT NULL UNIQUE`) — this introduces no schema change; the exporter is only
  choosing what string convention to store in a slot QMapShack already defines and, per a source
  read of every `keyqms` usage, treats purely as an opaque unique string for exact-match lookups,
  never parsed or shape-validated (see [`docs/qmapshack-format.md`](../qmapshack-format.md)). On
  each run, the exporter looks up existing items by this namespace to decide insert vs. update
  vs. remove.
- **Scoping — never touch what it didn't create.** Reconciliation (updates, removals, and any
  future folder cleanup) only ever considers `items`/`folders` rows whose `keyqms` carries the
  exporter's namespace prefix. Anything else in the target database — the owner's own manually
  added waypoints, routes, notes, or reorganization — is left completely untouched. Folders are
  created on demand and never auto-deleted, so an empty folder left behind by a moved/renamed
  trip doesn't risk deleting a folder the owner has repurposed.
- **Folder placement is owner-configured**, not a scheme invented by trip-archive. A small
  config (format/schema left as an implementation detail, e.g. TOML) maps trips to a folder
  path using trip attributes — at minimum activity type and year (e.g. illustratively
  `Trips/{year}/{activity_type}`, exact shape TBD during implementation). The exporter creates
  whatever folder path the config resolves to for each trip if it doesn't already exist
  (`folder2folder` chain) and places the trip's item under it (`folder2item`).
- **Rolling backup before write**: before making any change, the exporter copies the target
  database file to a timestamped backup (same directory, e.g.
  `<name>.backup-<timestamp>.db`), so a bad write — corruption, or a format assumption that
  turns out wrong — can be rolled back by restoring the most recent backup. As part of creating
  each new backup, the exporter first deletes older backups that are no longer required to
  satisfy the retention policy: keep enough backups to cover **at least one month** of run
  history, but **never fewer than three** backups regardless of run frequency (e.g. infrequent
  runs keep at least three even if that spans more than a month; frequent runs keep however many
  are needed to span a month, which may be more than three). This only guards against write-time
  corruption/crashes and gives a rollback path — it doesn't detect a silently-wrong-but-accepted
  blob (see below).
- **Bootstrapping a new target file**: if the target database doesn't exist yet, the (initial)
  export run creates it — full QMapShack schema (folders/items/hierarchy/FTS tables, per
  [`docs/qmapshack-format.md`](../qmapshack-format.md)) plus a `versioninfo` row matching the
  `DB_VERSION`/`VER_TRK` pair the exporter targets — rather than requiring the owner to
  pre-create an empty project by opening QMapShack first.
- **Compatibility gate**: before writing, the exporter checks the target database's
  `versioninfo.version` against the `DB_VERSION`/`VER_TRK` pair it was built for and refuses to
  proceed on a mismatch, with a clear error — never attempts to "migrate" the target database
  itself (that's QMapShack's own job, triggered by opening the file in QMapShack).
- **Locking**: for the duration of a run, the exporter takes the same global lock introduced by
  US-26 ([`requirements.md`](../requirements.md)) to block trip-archive operations that would
  change state while it runs — giving each run a consistent snapshot of the archive instead of
  risking a torn read against a concurrent edit or delete.
- **Track content**: `trk.segs`/points are derived from the same GeoJSON track geometry already
  stored per trip ([ADR-0003](./0003-track-as-geojson-blob-in-sqlite.md)); no re-parsing of the
  original GPX.
- **Field scope**: trip **photos are explicitly out of scope** — QMapShack items have no
  BlobStore-backed attachment concept, and reusing existing tracks (this feature's actual goal)
  doesn't need them. Every other trip attribute trip-archive holds (comment/description, tags,
  stats, etc., beyond name/activity_type/geometry already covered above) is exported on a
  **best-effort basis** into the closest matching QMapShack field as implementation allows — not
  an exhaustive mapping, but nothing besides photos is deliberately withheld.
- **Failure visibility**: the exporter reports outcome via process **exit code** (0 = clean run,
  non-zero = at least one failure) so an owner-configured cron job can alert on failure, and logs
  diligently — each insert/update/remove/skip decision and any error — so a failure can be
  diagnosed after the fact without re-running interactively.
- **Per-item, best-effort execution**: a run processes trips one at a time rather than as a
  single all-or-nothing transaction; a failure on one trip is logged and skipped, not fatal to
  the whole run. Because reconciliation is idempotent (see change-detection above), a subsequent
  run naturally retries/heals anything a prior run left inconsistent — no separate resume/retry
  logic needed.

## Consequences

- No new HTTP API surface; the exporter is a thin binary over existing repos, matching how
  `komoot_backfill` avoids duplicating the import pipeline.
- The export is **authoritative for trip-archive-owned items** — any manual edit the owner makes
  *inside QMapShack* to a trip-archive-exported item (recolor, rename, move to a different
  folder, add a history note) is silently overwritten on the next export run. This is an
  accepted trade-off of one-way sync with no round-trip, consistent with QMapShack being a
  side-tool only (per `docs/qmapshack.md`); the owner's own independently-created items are
  unaffected since reconciliation is scoped to the exporter's `keyqms` namespace.
- **Concurrent access risk**: QMapShack's sample databases use SQLite's default rollback-journal
  mode with `locking_mode=NORMAL` (no WAL) — running the export while the owner has the same
  database open in QMapShack can hit `SQLITE_BUSY`/"database is locked". Not mitigated in v1;
  the owner is expected to close QMapShack before running an export (documented, not enforced).
- Targeting an undocumented, reverse-engineered format carries the same class of risk as
  [ADR-0021](./0021-reverse-engineered-komoot-client.md)'s Komoot client: it can change without
  notice. Mitigated somewhat by the format's multi-year stability track record (see Context), by
  the `versioninfo` compatibility gate failing loudly rather than corrupting the file, and by the
  rolling backups above giving a rollback path — but unlike Komoot, there's no cheap "integration
  check" call to run proactively; a break would only surface when QMapShack refuses to open a
  freshly exported file, or behaves oddly with one that opens but was subtly misencoded (backups
  don't detect that, only let it be undone once noticed).
- **`keyqms` convention risk**: "any unique string works" is inferred from reading current
  source, not a documented contract — a future QMapShack version could start validating or
  parsing `keyqms` and reject a non-MD5-shaped key, or a namespace/id-scheme change on our side
  could orphan previously-exported rows (no longer matched by lookup, never cleaned up).
  QMapShack's MySQL backend (unused by us) caps the column at `VARCHAR(64)`; staying well under
  that length costs nothing and hedges against it.
- `QDateTime` byte-level encoding and the `folders.data` (`QMProj`) blob's inner structure are
  not yet empirically verified (see `docs/qmapshack-format.md`'s Open Items) — small remaining
  unknowns to close out during implementation, not expected to change the shape of this
  decision.
- Folder-mapping configuration format is deliberately left unspecified here; a follow-up (either
  an ADR addendum or just an implementation PR, given its small scope) will pin down the exact
  schema once the owner's requirements for it are fully known.
- Two smaller details are likewise deferred to implementation rather than decided here: how the
  exporter is told the **target database path** (CLI arg vs. config), and how `items.icon`
  (`NOT NULL`) gets populated — current thinking is extracting a real icon per activity type from
  a QMapShack-created example database rather than generating new artwork.
