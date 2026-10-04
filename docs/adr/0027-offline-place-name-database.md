# ADR-0027 — Offline geodata from OSM and national name registers

## Status

Accepted

## Context

US-74 suggests a trip name from the places its track passes — where it starts and ends, and its
main places. The suggestion is only worth offering if it is good: a name the owner has to rewrite
is worse than none. That needs typed, ranked names — summits, huts, lakes, bays, campsites,
villages, towns — at hiking-map density across Norway, Sweden, Finland, Germany and the Alps.
US-76 suggests an activity type from what the track runs on — water, roads and tracks, paths,
prepared ski trails — read from the same OpenStreetMap source over the same regions.

- **Online reverse geocoding** (Nominatim, commercial geocoders): rejected. These answer "the
  nearest address", not "the summit the track crossed"; they come with usage limits, some forbid
  storing results, and every lookup sends the track's coordinates to a third party.
- **Online feature queries** (Overpass, the national registers' APIs): good data, but every
  suggestion would depend on someone else's fair-use service and send the track away.
- **A self-hosted geocoder** (Nominatim, Photon): its resource needs do not fit scale-to-zero
  hosting ([ADR-0023](./0023-managed-scale-to-zero-hosting.md)).
- **An offline database derived from open data**: chosen. A spike over real trips produced good
  or acceptable names from OpenStreetMap for most of them, and Kartverket's register filled a gap
  where OSM had no name.

## Decision

Place names and the ground a track runs on (ways, water, ski trails) come from **read-only data
derived offline from open data**, queried locally by the server. **No coordinate leaves the archive
for a suggestion.**

- **Sources:** OpenStreetMap is the base for every covered region; national name registers
  supplement it where they are better and openly licensed.
- **The database is a build artefact, not archive data:** regenerated from its sources rather than
  migrated, and kept apart from the archive's own database and its backups.
- **It is optional at runtime:** without it, the name suggestion falls back to the one it replaces,
  the activity type gets no suggestion, and an import never fails because of it.
- **The suggestion is computed behind one server-side interface that takes the trip, not just its
  track.** Today only the track decides; other attributes of the trip — its activity type, say —
  can inform the ranking later without changing that interface.

## Consequences

- The data is far larger than the binary's embedded data — hundreds of MB for the place names
  alone, more with the ground data — and refreshing it means downloading and processing several GB
  of source data. Both change slowly, so an occasional manual refresh is enough.
- The sea is not a polygon in OpenStreetMap; the water polygons derived from its coastlines are an
  added source.
- All sources are used under open licences, and the app shows the attribution they require.
- Accepted data limits: a trip named after the route itself cannot be derived from places;
  multi-day wilderness stages get weak suggestions; quality follows the sources' coverage, which
  varies by region.
- Combining sources produces duplicates — the same place in two sources, or under a name in
  another language. Resolving them is part of the ranking, which is unit-tested against fixture
  data without network access ([ADR-0012](./0012-tdd-test-strategy.md)).
