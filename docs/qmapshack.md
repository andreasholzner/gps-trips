# Desktop GIS Integration (QMapShack / QGIS) — Discussion Notes

**Status:** exploratory discussion log, kept for context. The outcome has since graduated into
formal docs: user stories **US-36/US-37** in [`requirements.md`](./requirements.md), the design
decision in [ADR-0022](./adr/0022-qmapshack-export.md), and the byte-level format spec in
[`qmapshack-format.md`](./qmapshack-format.md). This file remains as the narrative trail of how
that outcome was reached.

## Goal

Easily compare different trips visually, and plan new routes by reusing parts of existing
trips, using desktop GIS tools.

## Tool split

- **QMapShack** — the primary tool for this use case. Purely a side-tool for now: no
  round-trip back into the archive. A newly planned route (spliced from existing tracks)
  would still be imported into Komoot the established way, not back into the archive
  directly. Round-tripping could revisit US-31 (recorded vs. planned import) later.
- **QGIS** — considered but not the fit for this use case. No built-in "cut this segment
  into a new route" workflow; more relevant for spatial analysis (overlap detection,
  heatmaps) than route splicing. Not pursued further in this discussion.

## Export format options considered

### 1. Bulk/filtered GPX export
- Reuses existing per-trip GPX download (US-21) and existing filters (US-13: activity,
  date, region, name).
- Standard, documented, stable format — QMapShack reads it natively, zero format risk.
- **Rejected as insufficient at scale.** With ~1000 recorded trips, flat GPX files (whether
  one-per-trip or combined multi-track) have no organizational concept. Folder-based
  structure on disk and multi-track GPX were both judged hard to maintain at this scale.
  Importing many individual GPX files into QMapShack is also less comfortable than a
  single import action.

### 2. QMapShack native SQLite database export
- QMapShack has a native database/library feature (SQLite-backed) intended for browsing
  large personal track collections, with folder/category organization — a real capability
  gap that plain GPX cannot close at this scale.
- Enables a single-import experience instead of importing ~1000 individual files.
- **Risk:** this is QMapShack's internal format, not a documented interchange standard.
  Being open source makes the schema *discoverable* but not stable or versioned in any
  guaranteed way — it could change between QMapShack releases without notice. Building
  against it means reverse-engineering rather than integrating against a spec.

## Feasibility spike (done)

Inspected a small hand-crafted QMapShack database (`docs/qmapshack_analysis/format_test.db`;
3 tracks, nested folders, one "other" item) via Python's `sqlite3`. A larger real-world
database (`Touren.db`, ~1000 items) was intentionally left unopened for a later consistency
check.

### Schema — low risk

Plain, well-normalized relational schema:

- `folders` (id, type, name, comment, sortmode, …) and `items` (id, type, name, comment,
  icon, data, hash, last_user, last_change, trash) hold the metadata.
- `folder2folder` / `folder2item` are junction tables giving an arbitrary folder tree —
  exactly the folder/category organization GPX can't express.
- Triggers (`folder2item_insert/delete`, `items_update_last_change`, `searchindex_insert/
  update`) automatically maintain the trash-bin logic and the FTS4 full-text search index
  (`searchindex*` tables) on plain `INSERT`/`UPDATE`/`DELETE` — a writer doesn't need to
  replicate that bookkeeping by hand.
- `icon` is a plain PNG blob — trivial.
- A `versioninfo` table explicitly stores `('6', 'QMapShack')` — a usable compatibility
  guard (check/require this value before writing). Note `PRAGMA user_version` is unused
  (always 0); QMapShack tracks its own version out-of-band from SQLite's mechanism.

This part alone would be a comfortable, low-risk implementation.

### `items.data` blob — high risk

This is the one genuinely proprietary piece, and it's where the real effort/risk lives.
For a track item it's a custom binary container, not GPX and not a documented format:

1. An **uncompressed header/history section** — Qt `QString`-style fields (4-byte
   length-prefixed UTF-16), e.g. an edit-history entry containing an icon resource path
   (`//icons/48x48/Start.png`) and a comment (`Initial version.`).
2. A `QMTrk` magic marker + padding + several version/length fields, followed by a
   **zlib-compressed payload** (a 2172-point track compressed ~36KB → decompresses to
   ~237KB) holding the actual track geometry, timestamps, and styling (e.g. line color
   `DarkRed`), all in Qt's binary (`QDataStream`) serialization.
3. An **uncompressed trailer** duplicating the `hash` and `last_user` values that are
   *also* present as plain SQL columns — i.e. the relational columns aren't fully
   authoritative on their own; QMapShack's C++ loader reconstructs the object from the
   blob.

Qt's `QDataStream` wire format itself is documented/stable, but the specific object
layout (history list → `QMTrk` chunk → trailer, and the track/point/style structure
inside the compressed chunk) is QMapShack-internal and undocumented. Producing a blob
QMapShack accepts means porting the relevant slice of QMapShack's C++ serialization code
(`CGisItemTrk` and friends) to Rust byte-for-byte, not just matching a spec.

### Feasibility & risk assessment

- **Feasible**, in the sense that nothing is encrypted or opaque — every byte was
  inspectable with a generic SQLite + zlib toolchain, and Qt's serialization primitives
  are well-known.
- **Effort is materially higher than hoped.** The original 1-hour spike estimate covered
  the relational schema fine, but the item payload is a versioned custom binary format
  requiring a hand-written Qt-compatible serializer, reverse-engineered from QMapShack's
  source rather than the DB alone. This is a multi-day effort, not a weekend one, with
  ongoing risk of drift if QMapShack changes its object layout (the `versioninfo` value
  would presumably bump — worth confirming against QMapShack's changelog/source before
  committing).
- **Mitigating option not yet explored:** check whether QMapShack's loader for `items.data`
  accepts anything simpler (e.g. a raw GPX byte string) as an alternative payload for a
  track item — would need reading QMapShack's C++ load path, not just its DB. No evidence
  either way yet.

## Format-change history (checked against QMapShack source, github.com/Maproom/qmapshack)

Cloned the upstream repo and walked the git history of the two version constants that
gate compatibility: `DB_VERSION` (`src/qmapshack/gis/db/macros.h`, the relational schema
— matches the `versioninfo` table) and `VER_TRK` (`src/qmapshack/gis/qms/serialization.cpp`,
the internal version tag inside the `items.data` track blob).

**`DB_VERSION` (relational schema):**

| Version | Date first shipped |
|---|---|
| 1 | 2014-11-04 |
| 2 | 2015-12-06 |
| 3 | 2015-12-13 |
| 4 | 2016-01-03 |
| 5 | 2016-07-04 |
| 6 | 2016-07-19 |

No change since **2016-07-19** — stable for ~10 years, through the current `V_1.20.3`
release.

**`VER_TRK` (track blob internal format):**

| Version | Date first shipped |
|---|---|
| 1 | 2014-10-24 |
| 2 | 2015-11-04 |
| 3 | 2016-01-09 |
| 4 | 2016-01-15 |
| 5 | 2016-01-29 |
| 6 | 2018-11-02 |
| 7 | 2020-01-03 (shipped in release `V_1.14.1`, 2020-03-27) |

No change since **2020-01-03** — stable for ~6.5 years, through the current `V_1.20.3`
release. (One same-day 6→5→6 revert-and-refix on 2019-03-03 was excluded as noise —
net no change.)

**Reading:** both formats changed frequently only in the project's first ~2–5 years
(2014–2020), then went stable — no breaking changes in 6+ years despite ongoing commits
to the same files (refactors, formatting passes) since then. This meaningfully de-risks
the "chasing an upstream format" concern: a writer targeting the current format
(`DB_VERSION 6`, `VER_TRK 7`) is very unlikely to need rework soon, though there's no
guarantee — only precedent.

**Also confirmed (`IDB::setupDB` in `IDB.cpp`):** QMapShack auto-migrates a DB with an
*older* `versioninfo.version` (with user confirmation), but flatly **refuses to open** a
DB with a *newer* version than the installed QMapShack understands ("Database created by
newer version of QMapShack"). Practical implication: an export should target the
`DB_VERSION`/`VER_TRK` pair matching the user's *installed* QMapShack release, not
blindly the latest upstream `dev` branch — worth pinning to the release actually in use
before writing anything.

## Exact `items.data` blob structure (resolved)

Read the actual save path in the cloned source
(`docs/qmapshack_analysis/qmapshack-src/src/qmapshack/gis/qms/serialization.cpp` and
`.../gis/db/CDBProject.cpp`). The earlier "history section → QMTrk chunk → trailer"
description was a reasonable read of the raw bytes but not quite the real structure.
What's actually happening:

- `items.data` = serialized `IGisItem::history_t`: `[VER_HIST, histIdxInitial: qint32,
  histIdxCurrent: qint32, events: QVector<history_event_t>]`.
- Each `history_event_t` = `[VER_HIST_EVT, time: QDateTime, icon: QString, comment:
  QString, data: QByteArray, hash: QString, who: QString]`.
- The `data` field of an event is itself the full `QMTrk`-tagged, `qCompress`-compressed
  serialization of the item's type-specific content at that point in its edit history
  (`CGisItemTrk::operator>>`). The "trailer" bytes previously found right after the
  compressed chunk are simply that same event's next two fields, `hash` and `who` — not
  a separate structure. A freshly-created item just needs **one** history event (e.g.
  comment "Initial version.", `who = "QMapShack"` or similar).
- `qCompress`/`qUncompress` is Qt's own format: a 4-byte big-endian uncompressed-length
  prefix followed by a plain zlib (deflate) stream — not QMapShack-specific, documented
  Qt behavior.
- Both the outer stream (`CDBProject::insertItem`) and the inner one (`CGisItemTrk::
  operator>>`) explicitly pin `QDataStream::LittleEndian` byte order and
  `QDataStream::Qt_5_2` stream version — so the primitive wire format (how a `QString`,
  `QVector`, `double`, etc. are encoded) is a **fixed, documented Qt format**, independent
  of whatever Qt version QMapShack itself is built against. That removes a whole axis of
  uncertainty: a writer only needs to match Qt 5.2's `QDataStream` encoding once, not
  chase a moving target.
- The track's own field list (`CGisItemTrk::operator>>`, ~25 fields: name/cmt/desc/src,
  links, number, type, color, rating, keywords, colorSourceLimit, lineScale, showArrows,
  three graph limit structs, cycling-energy settings, and `trk.segs`) is read with
  `if (version > N)` guards per field — i.e. VER_TRK's version history (see above) is
  exactly the field list growing over time. Targeting VER_TRK 7 means writing the full
  current field set once, with no legacy-format branching needed since we only ever write
  new files. `trk.segs` is `QVector<trkseg_t>`, each holding `QVector<trkpt_t>`, each
  point built on the same `wpt_t` fields already seen for waypoints (lat, lon, ele, time,
  …) plus point flags/extensions/activity.

Net effect on the risk assessment: the format is now **fully specified**, not merely
"inspectable." The remaining work is porting a well-defined (if sizeable) struct list to
a new writer, testable byte-for-byte against the known-good reference file
(`format_test.db`) already in hand.

## Implementation approach: Rust-native vs. a separate C++/Qt sync tool

Two shapes were considered for whoever ends up writing this blob:

**(a) Rust-native, inside trip-archive** — a new writer (likely a separate CLI binary in
the workspace, following the existing `komoot_check`/`komoot_backfill` precedent) that
implements Qt's `QDataStream` (Qt_5_2, little-endian) primitives and `qCompress` framing
directly in Rust, then encodes the struct layout above.

**(b) A separate C++/Qt sync program** that queries trip-archive through a (new) API and
writes the QMapShack DB using actual Qt (`QDataStream`, `qCompress`) — the user's framing
being that this could reuse QMapShack's own serialization code and avoid reimplementing
Qt's wire format.

Reading the source changes the calculus on (b): `CGisItemTrk`/`IGisItem` are not a clean,
headless, reusable library. They're entangled with QMapShack's GUI layer — e.g.
`IDB::setupDB()` (core DB open/migrate logic) directly pops a `QMessageBox` on a version
mismatch, and item icons are handled via `QPixmap`. Reusing QMapShack's actual classes
would mean pulling in a large chunk of QtWidgets and picking apart GUI-coupled code paths,
not linking a small library. So (b)'s main promised benefit — reusing QMapShack's code —
mostly doesn't materialize; a C++/Qt tool would still hand-port the same struct layout
described above, just in a different language. What it *would* get for free is Qt's own
correct implementation of `QDataStream`/`qCompress` framing, removing the "did I replicate
Qt's primitive encoding exactly right" risk category — but that risk is now small anyway,
since the format is documented/pinned (`Qt_5_2`) and testable byte-for-byte against
`format_test.db`.

Weighed against that small remaining edge, (b) adds real cost: a second language/toolchain
(C++/Qt dev libraries) in a project whose stated driver is self-contained single-binary
deployment and being a Rust-learning project (`docs/requirements.md`), a new deployable
artifact, and — per the user's own framing — a new API surface to avoid coupling to
internal implementation details, which is extra design/build work regardless of language.

**Current lean: (a).** The primitive-encoding risk that would justify reaching for real
Qt is now testable and bounded (compare Rust output against `format_test.db` byte-for-
byte), so the main advantage of (b) is smaller than it first looked, while its costs
(second toolchain, second deployable, off the project's stated single-binary/Rust-learning
path) are concrete and immediate. The one part of (b)'s framing worth keeping regardless
of language: don't have the exporter read internal SQL tables directly — go through a
stable interface (existing JSON API per ADR-0008, or the same domain/service layer the
`komoot_*` binaries already use) so the writer isn't coupled to trip-archive's schema
internals either.

## Consistency check against the real `Touren.db` (done)

`Touren.db` (the real, ~40MB production database, previously left unopened) has 4584
items (2791 waypoints, 1792 tracks, 1 route, 0 areas) across 53 folders, `versioninfo`
`('6', 'QMapShack')` — same schema as the hand-built test file.

Verified a stratified random sample of 17 items (6 tracks, 10 waypoints, 1 route — the
only route in the DB) with a small script that, per item: locates the type-specific magic
marker, reads the version byte, decompresses the `qCompress`-framed payload (4-byte
big-endian uncompressed-length prefix + zlib stream) and checks the decompressed length
against the declared one, then reads the two trailing `QString` fields (`hash`, `who`)
and cross-checks them against the `items.hash`/`items.last_user` SQL columns.

**Result: 17/17 clean.** Every sampled item's magic marker matched its `items.type`,
every version byte matched the current source (`VER_TRK=7`, `VER_WPT=4`, `VER_RTE=4`),
every payload decompressed cleanly with the declared length, every trailing hash/who
string matched the corresponding SQL columns exactly, and every icon blob was a valid
PNG. Folder blobs sampled all carried the `QMProj` magic, consistent with the test file.
One incidental observation, not a format concern: 4276 of 4584 items are linked via
`folder2item` — the remaining 359 are presumably in the trash bin (per the
`folder2item_delete` trigger setting `items.trash`), not evidence of a different format.

This confirms the structure resolved above (`history_t` → single/multiple
`history_event_t` → embedded `QMTrk`/`QMWpt`/`QMRte` chunk → trailing `hash`/`who`)
generalizes from the 3-item hand-built file to a real ~4600-item, multi-year database,
across all three item types actually in use. Nothing here suggests the earlier reasoning
was fitted to the toy example.

## Open questions

- If pursued, should export be full-library (all ~1000 trips) or filtered (reuse US-13
  filters) into the QMapShack DB?
- Not yet decided: which existing interface (JSON API vs. shared domain/service crate,
  following the `komoot_check`/`komoot_backfill` CLI-binary pattern) the exporter should
  use to read trip data.
