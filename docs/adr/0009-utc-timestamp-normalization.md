# ADR-0009 — Normalize timestamps to UTC; document the EXIF-offset assumption

## Status

Accepted

## Context

Placing non-geotagged photos on the track ([US-4](../requirements.md)) requires comparing photo
timestamps with track point timestamps. But the two sources use different time conventions:

- **GPX `<time>`** is UTC (RFC 3339, `Z`).
- **EXIF `DateTimeOriginal`** is local wall-clock with **no timezone**.

Mismatched zones are the most likely cause of photos being pinned at the wrong point on the track.

## Decision

Normalize **everything to UTC** internally. When a photo provides `OffsetTimeOriginal`, use it.
Otherwise, assume a **configured trip-local UTC offset**, record the assumption, and document this
behavior. Time-matching binary-searches the track's UTC timestamps and linearly interpolates
position between the bracketing points; photos whose time falls outside the track range are left
unplaced (`location_source = none`).

## Consequences

- Reliable time-matching when the offset is known or present in EXIF.
- A documented, predictable failure mode when the offset is unknown (rather than silent
  mis-placement).
- The assumed offset is a per-import (or configurable) input, not a hidden constant.

## Addendum (US-4 implementation)

The configured trip-local UTC offset is represented as a per-trip **IANA timezone name**, rather
than a raw UTC-minutes number, auto-guessed from the track's start coordinate via
[ADR-0019](./0019-tzf-rs-time-tz-for-timezone-lookup.md)'s timezone lookup, with an optional
owner override. There is no mechanism to edit a trip's timezone after import, nor to retroactively
recompute photos already placed.
