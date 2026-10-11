# ADR-0028 — Collections are records of their own, apart from trips

## Status

Accepted

## Context

A guide book's tracks and waypoints ([US-86](../requirements.md)) are suggestions from someone
else, not trips the owner planned or did. Whatever is about the owner's trips must never count
them as trips, and a guide-book track never has much of what a trip has.

## Decision

**Collections, their tracks and their waypoints are records of their own, not trips.**

- **A collection owns its tracks and waypoints.** A waypoint may also belong to one of the
  collection's tracks. Deleting a collection deletes everything it owns.
- **Tags and shares reach collections and collection tracks through links of their own**, beside
  the trips' ones, so removing a collection, a track, a tag or a share removes its links as it does
  for trips.
- **What is not about the trip record itself is shared, not duplicated:** reading GPX, a track's
  figures and geometry, and the views that show them.

## Consequences

- Nothing that reads trips sees a collection or its tracks unless it asks for them, including what
  is added later.
- Operations a trip and a collection track both have (viewing, editing, deleting, downloading) exist
  once for each, over shared parts.
- Tags and shares cover more than one kind of thing.

## Alternatives considered

- **Collection tracks as trips of a new kind.** Most of the trip pages and endpoints would work as
  they are, but everything that reads trips, now and later, would have to leave them out, and much
  of what a trip records would never apply to them.
