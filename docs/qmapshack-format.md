# QMapShack database format — reference

This is a **living reference doc**, not an ADR: it records the on-disk format of a QMapShack
SQLite database so an implementer doesn't have to re-derive it. The *decision* to export into
this format, and the architecture built around it (export CLI binary, folder-mapping config,
one-way reconciliation), lives in
[ADR-0022](./adr/0022-qmapshack-export.md) and links back here for the details.

Unlike an ADR, this doc is expected to change if QMapShack's format changes, or as
implementation fills in details left open below — not just at decision points.

## Source / provenance

QMapShack has no documented export/interchange format for its database — this was derived by
inspecting real `.db` files and reading QMapShack's own C++ source
(`github.com/Maproom/qmapshack`, vendored read-only at
`docs/qmapshack_analysis/qmapshack-src/` for reference — gitignored, re-clone if missing).
Verified against:
- A small hand-built test file (`docs/qmapshack_analysis/format_test.db`, 3 tracks).
- A real production database (`docs/qmapshack_analysis/Touren.db`, 4584 items) — a stratified
  random sample of 17 items (tracks, waypoints, the one route) matched the structure below
  byte-for-byte, cross-checked against the source at
  `src/qmapshack/gis/qms/serialization.cpp` and `src/qmapshack/gis/db/{IDB,IDBSqlite,
  CDBProject}.cpp`.

Treat anything not explicitly marked "verified" below as read-from-source but not yet
byte-level confirmed.

## Compatibility gate

`versioninfo` table: `(version TEXT, type TEXT)`, one row, e.g. `('6', 'QMapShack')`. Must match
`DB_VERSION` (`src/qmapshack/gis/db/macros.h`) of the QMapShack release the owner actually runs.
QMapShack auto-migrates a DB with an *older* version (with a user prompt) but **flatly refuses
to open** one with a *newer* version than it understands (`IDB::setupDB`, `IDB.cpp`). An
exporter must target the version matching the owner's installed release, not just upstream
`dev`. `PRAGMA user_version` is unused by QMapShack (stays 0) — don't rely on it.

As of this doc: `DB_VERSION = 6` (stable since 2016-07-19), track item format `VER_TRK = 7`
(stable since 2020-01-03), waypoint `VER_WPT = 4`, route `VER_RTE = 4`. See
[ADR-0022](./adr/0022-qmapshack-export.md) for the full change-frequency history.

## Relational schema

```sql
CREATE TABLE folders (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  type INTEGER NOT NULL,       -- folder-type enum, see IGisItem.h
  keyqms TEXT,
  date DATETIME DEFAULT CURRENT_TIMESTAMP,
  name TEXT NOT NULL,
  comment TEXT,
  locked BOOLEAN DEFAULT FALSE,
  data BLOB,                   -- QMProj-magic blob, see below
  sortmode INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  type INTEGER,                -- 1=waypoint, 2=track, 3=route, 4=area (IGisItem::type_e)
  keyqms TEXT NOT NULL UNIQUE, -- item identity key, see "Item identity" below
  date DATETIME DEFAULT CURRENT_TIMESTAMP,
  icon BLOB NOT NULL,          -- plain PNG bytes
  name TEXT NOT NULL,
  comment TEXT,
  data BLOB NOT NULL,          -- history_t blob, see below
  hash TEXT NOT NULL,          -- must equal the hash embedded in `data`'s current event
  last_user TEXT DEFAULT 'QMapShack',
  last_change DATETIME DEFAULT CURRENT_TIMESTAMP,  -- auto-set by trigger on UPDATE
  trash DATETIME DEFAULT NULL  -- auto-managed by folder2item triggers, see below
);

CREATE TABLE folder2folder (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  parent INTEGER NOT NULL REFERENCES folders(id),
  child INTEGER NOT NULL REFERENCES folders(id)
);

CREATE TABLE folder2item (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  parent INTEGER NOT NULL REFERENCES folders(id),
  child INTEGER NOT NULL REFERENCES items(id)
);
```

Plus an FTS4 full-text index (`searchindex` + its shadow tables) over `items.comment`, kept in
sync automatically by triggers — never write to it directly.

**Triggers do real bookkeeping automatically** — an external writer gets this for free via plain
`INSERT`/`UPDATE`/`DELETE`:
- `items_update_last_change` — sets `items.last_change = CURRENT_TIMESTAMP` on any `UPDATE`.
- `folder2item_insert` — clears `items.trash` when an item is (re)linked into a folder.
- `folder2item_delete` — sets `items.trash = CURRENT_TIMESTAMP` when an item's last
  `folder2item` row is removed (i.e. **unlinking from all folders is how you "delete" an item**
  — it moves to QMapShack's trash, it doesn't disappear from the `items` table).
- `searchindex_insert` / `searchindex_update` — keep the FTS index in sync with `items.comment`.

## `items.data` blob (verified)

`items.data` is a serialized `IGisItem::history_t`:

```
history_t:
  quint8   VER_HIST (= 1)
  qint32   histIdxInitial
  qint32   histIdxCurrent
  QVector<history_event_t> events   -- quint32 count, then elements
```

```
history_event_t:
  quint8    VER_HIST_EVT (= 3)
  QDateTime time     -- not yet byte-verified, see "Open items" below
  QString   icon      -- e.g. "//icons/48x48/Start.png"; cosmetic, any resource path works
  QString   comment   -- e.g. "Initial version."
  QByteArray data      -- the type-specific chunk, see below
  QString   hash       -- must equal items.hash for the row's *current* event
  QString   who        -- e.g. "QMapShack"; free text, matches items.last_user
```

A freshly-created item needs exactly **one** history event (`histIdxInitial = histIdxCurrent =
0`, one-element `events`).

The event's `data` field is itself a magic-tagged, compressed chunk:

```
type-specific chunk (e.g. for a track):
  char[10]  magic       -- "QMTrk     " (5 chars + 5 spaces of padding), or
                         --   "QMWpt     " (waypoint) / "QMRte     " (route) / "QMArea    " (area)
  quint8    VER_TRK (= 7 currently; VER_WPT=4, VER_RTE=4, VER_AREA=2)
  QByteArray qCompress(<inner fields>, 9)
```

`qCompress`/`qUncompress` is **Qt's own framing**, not QMapShack-specific: 4 bytes big-endian
uncompressed length, followed by a plain zlib (deflate) stream. Documented Qt behavior.

The inner (pre-compression) fields for a track (`CGisItemTrk::operator>>`,
`src/qmapshack/gis/qms/serialization.cpp`) are written with an **explicitly pinned** encoding —
`QDataStream::LittleEndian`, `QDataStream::Qt_5_2` — for both this inner stream and the outer
`history_t` stream. This fixes the primitive wire format (`QString`, `QByteArray`, `QVector`,
numeric types) to one specific, documented Qt encoding regardless of which Qt version the
reading QMapShack build uses.

Field **names and order** for a track (verified present in source; exact primitive C++ types —
`qreal` vs `float`, `quint8` vs `quint16`, etc. — should be read directly from source at
implementation time rather than trusted from this summary):

```
key.item, flags, trk.name, trk.cmt, trk.desc, trk.src, trk.links (QVector<link_t>),
trk.number, trk.type, trk.color, rating, keywords, colorSourceLimit, lineScale, showArrows,
limitsGraph1, limitsGraph2, limitsGraph3, energyCycling (energy_set_t), trk.segs
```

`trk.segs` is `QVector<trkseg_t>`; each `trkseg_t` is `[VER_TRKSEG, QVector<trkpt_t> pts]`; each
`trkpt_t` is `[VER_TRKPT, flags, <wpt_t base fields>, extensions, activity]`, where `wpt_t` (also
the top-level payload for a waypoint item) is `[lat, lon, ele, time, magvar, geoidheight, name,
cmt, desc, src, links, sym, type, fix, sat, hdop, vdop, pdop, ageofdgpsdata, dgpsid]`.

Exact source locations for implementation: `serialization.cpp` lines ~324-400 (segment/point),
~492-620 (track), ~60-100 (waypoint base fields), ~124-165 (history).

## Item identity (`keyqms`)

`items.keyqms` is `TEXT NOT NULL UNIQUE`. QMapShack itself generates it as an MD5 hex hash
(`IGisItem.cpp`), but nothing in the schema or the reader enforces that shape — no format
validation was found. Safe to use any unique, stable string. See ADR-0022 for how the exporter
uses a namespaced, deterministic `keyqms` (derived from the trip's own id) to identify which
items it owns across re-runs.

## `folders.data` blob

Present for folders with a `QMProj` magic marker (10-byte padded, like the item magics above),
followed by a small Qt-serialized structure (not fully decoded — folders written by the exporter
don't strictly need this populated; `data` is nullable per the schema. Confirm at
implementation time whether QMapShack requires it for folders it didn't create itself).

## `icon` blob

Plain PNG bytes, shown in QMapShack's item tree/list. No need to replicate QMapShack's own
icon-rendering logic — embedding one or a few static PNG resources (e.g. per activity type) is
sufficient.

## Open items (not yet resolved)

- **`QDateTime` byte encoding** — not empirically decoded. Standard Qt behavior under stream
  version `Qt_5_2` (documented by Qt), but verify against a real sample before relying on it.
- **`folders.data` (`QMProj`) blob** — magic marker confirmed, inner structure not decoded. Only
  matters if QMapShack turns out to require a populated blob for folders it didn't create.
- **Concurrent access** — sample databases use SQLite's default rollback-journal mode (`PRAGMA
  journal_mode = delete`), and QMapShack sets `locking_mode=NORMAL`. Standard SQLite file
  locking applies: writing to the file while QMapShack has it open can hit `SQLITE_BUSY`/
  "database is locked". No WAL mode in play.
