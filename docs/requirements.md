# Trip Archive — Requirements & User Stories

## Purpose

A personal, **single-user, self-hosted** archive for organizing trips (GPS tracks + photos)
and browsing them on a map. It replaces **komoot's organization features only**.

- **In scope:** importing GPS tracks, attaching photos, placing photos on a map, browsing
  trips with stats, reliving a trip (map + elevation + gallery).
- **Out of scope (stays in komoot):** recording tracks, route planning, discovery/highlights,
  social features.

The overarching driver: *"self-host the whole thing so I own my data."* and a learning opportunity for rust and geo data

## Actors

- **The owner** — the single user who owns and runs the instance. There are no other roles in v1.

## v1 User Stories (in scope)

**State:** ✅ done · 🚧 in progress · 📋 planned

| ID | State | Story | Acceptance criteria |
|----|:-----:|-------|---------------------|
| **US-1** | ✅ | As the owner, I import a **GPX file** (exported from komoot or my recorder) so a trip is archived outside komoot. | Uploading a valid GPX creates a trip and redirects to its detail page. Invalid/empty GPX is rejected with a clear error. |
| **US-2** | ✅ | As the owner, I **attach photos** to a trip so they are stored alongside the track. | Photos uploaded with the import are stored and associated with the trip. Photos can be added to a trip both during the gpx import and at a later time. |
| **US-3** | ✅ | As the owner, photos with **EXIF GPS** appear on the map where they were taken. | A geotagged photo shows a marker at its EXIF coordinates; `location_source = exif`. |
| **US-4** | ✅ | As the owner, photos **without GPS** are placed by matching their timestamp to the track, so untagged shots still appear. | A non-geotagged photo whose time falls within the track range gets an interpolated position (`location_source = interpolated`). A photo outside the track time range is left unplaced (`location_source = none`) and not shown on the map. |
| **US-5** | ✅ | As the owner, **thumbnails** are generated automatically on import so galleries and maps load fast. | Each photo has a generated thumbnail; originals are kept untouched; EXIF orientation is honored. |
| **US-6** | ✅ | As the owner, I **browse a list of all trips** with summary stats so I can scan my history. | List shows each trip's name, date, distance, ascent, and duration; loads without reading track geometry. |
| **US-7** | ✅ | As the owner, a **trip detail page** lets me relive a trip. | Shows the track on an OSM map, an elevation profile, and a photo gallery with map markers. *(Map + elevation + gallery done; photo map markers land with US-3/US-4.)* |
| **US-8** | ✅ | As the owner, trip **stats are computed automatically**, never entered by hand. | Distance, ascent, descent, duration, and bounding box are derived from the GPX at import. |
| **US-9** | ✅ | As the owner, I can **delete a trip** (and its files) to fix mistakes. | Deleting a trip removes its DB rows (cascade) and its photo blobs; no orphaned files remain. |
| **US-10** | ✅ | As the owner, I **self-host** the whole thing on my own machine. | Single deployable binary + static assets; all data under a configurable data directory (`TRIP_ARCHIVE_DATA_DIR`); no external services required. |
| **US-11** | 📋 | As the owner, when importing a **GPX file** I choose an activity type for the trip (e.g. cycling, hiking, ...). | The activity type is stored in the database and shown on the list over all trips and on the trip detail page. |
| **US-12** | 📋 | As the owner, when importing a **GPX file** I choose a name for the trip with a automatically suggested prefix of the trip date in the format `YYYY-mm-dd`. | The trip date is suggested as prefix for the name in the format `YYYY-mm-dd` and saved to the database. |
| **US-13** | 📋 | As the owner, I can filter the list of my trips by activity type, date interval, distance and free search of the name. | List shows only trips matching the selected filter criteria. |
| **US-14** | 📋 | As the owner, I can filter the list of my trips by geographic region by selecting an area in a map. | List shows only trips matching the selected region. |
| **US-15** | 📋 | As the owner, I can edit trip details (name and activity type) from the **trip details page** to correct mistakes. | The new values for name and activity type are saved to the database. |
| **US-21** | ✅ | As the owner, I can **download the original GPX file** I imported, from the trip detail page, so I keep an untouched copy of the source. | The exact uploaded GPX bytes are stored on import; the detail page offers a download link; downloading returns the original file byte-for-byte with `Content-Type: application/gpx+xml` and a sensible filename. |


## Future User Stories (must not be precluded by v1)

| ID | State | Story | Notes |
|----|:-----:|-------|-------|
| **US-16** | 📋 | As the owner, I access my archive from **Android**. | PWA first (cheapest; recording stays in komoot, so no native sensors needed); native app possible later against the JSON API. |
| **US-17** | 📋 | As the owner, my photos live on my private **ownCloud** instance. | Photos move to an `OwnCloudWebDav` storage backend; the SQLite DB (incl. tracks) stays local. |
| **US-18** | 📋 | As the owner, I import trips from **Garmin Connect**. | Plugs into the same import pipeline as an alternate ingestion source. |
| **US-19** | 📋 | As the owner, I can put a **single shared password** in front of my instance. | Optional auth middleware; no multi-user accounts. |
| **US-20** | 📋 | As the owner, my changes of name and activity type can be synced to komoot. | Optional sync of name and activity type to my komoot account. |
| **US-21** | 📋 | As the owner, I can manually place attached photos on a track location. | Photos can be placed by selecting a point on the map, the track is shown when selecting a new location. Automaticly determined locations (exif or interpolated) can be overwritten manually after a warning. | 

> **Deployment is intentionally deferred** — the app runs **laptop-local, on demand** for now.
> The phone/cloud/ownCloud topology will be chosen later from real use; see
> [ADR-0014](./adr/0014-defer-deployment-topology.md) for the decision and its revisit triggers.

## Non-functional requirements

- **Self-contained:** no external API keys (OSM raster tiles, no key); single binary + assets.
- **Data ownership & portability:** all state under one data dir; SQLite DB is a single-file backup.
- **Performance at personal scale:** designed for one user's trips (hundreds), each track up to
  tens of thousands of points; list views must not load track geometry.
- **Reliability:** imports are transactional (a failed import leaves no partial trip).
- **Maintainability:** server-only code isolated from the WASM client; small JS surface for maps/charts.

## Traceability (stories → key decisions)

- US-1/US-8 → ADR-0004 (import handler), ADR-0003 (track storage)
- US-21 → ADR-0003 (original GPX stored in DB with the track), ADR-0008 (download endpoint)
- US-2 → ADR-0004 (import + add-photos-later via the same pipeline)
- US-3/US-4 → ADR-0009 (timezone normalization)
- US-5 → ADR-0007 (BlobStore holds originals + thumbnails), ADR-0020 (image crate for thumbnail generation)
- US-7 → ADR-0005 (Leaflet/OSM), ADR-0006 (uPlot elevation chart)
- US-11/US-15 → ADR-0008 (write API: import metadata + edit endpoint)
- US-13/US-14 → ADR-0011 (filtering, search & geographic queries on SQLite)
- US-10 → ADR-0002 (local SQLite), ADR-0010 (self-hosted, optional auth), ADR-0016 (assets relative to executable)
- US-16 → ADR-0008 (JSON-first API enables a future client)
- US-17 → ADR-0007 (BlobStore enables ownCloud), ADR-0002 (DB stays local)
- US-19 → ADR-0010 (optional shared-password auth)

See `adr/` folder for the full Architecture Decision Records and
[`architecture.md`](./architecture.md) for the C4 diagrams. The original
[`initial_plan.md`](./initial_plan.md) is kept as a frozen historical snapshot.
