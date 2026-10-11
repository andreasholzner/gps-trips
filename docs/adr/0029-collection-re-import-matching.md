# ADR-0029 — A collection's tracks and waypoints are matched on re-import, and the owner decides

## Status

Accepted

## Context

A guide book comes as several files, and publishers issue corrected ones
([US-88](../requirements.md)). Adding a file to a collection must not duplicate what the collection
already holds. Exact comparison doesn't work: an edited or re-exported file rounds or slightly
shifts its points. Any automatic merge would sometimes be wrong about a guide book the owner knows
better than the archive does. Collection tracks also carry links of their own (tags, shares, an
activity override; [ADR-0028](./0028-collections-apart-from-trips.md)) that a careless
replacement would break.

## Decision

**When files are added to a collection, the server matches their contents against what the
collection holds and proposes; the owner decides; nothing changes before the owner confirms.**

- **Same means the same name with geometry, or position, within a tolerance.** A track matches
  when its line and the held line lie within a distance of each other throughout. A waypoint
  matches when its position is within a distance. An item without a name is matched on geometry or
  position alone. The tolerances are configuration, not part of this decision.
- **A matched item that differs is a changed item, not a new one.** Everything except the name,
  geometry or position counts as a difference.
- **The server only proposes.** Every proposal can be changed by the owner. The confirmed outcome
  is applied all at once or not at all.
- **A replaced item keeps its identity.** It is updated in place, so its links and its own
  settings survive.

## Consequences

- Re-importing never produces duplicates the owner didn't choose, and never removes anything the
  owner didn't choose to remove.
- A file with rounded or shifted points is recognised. Two genuinely different items with the same
  name that lie within the tolerance are taken for one; the owner can override the proposal.
- Anything that later brings tracks or waypoints into an existing collection (such as a future
  source other than a file) uses this matching rather than its own.
- Links to collection tracks and waypoints can rely on an item's id outliving a corrected file.

## Alternatives considered

- **Exact comparison of points.** Simple, but a re-exported file would duplicate every track.
- **Matching by name alone.** Breaks for unnamed items and for books that reuse names such as
  "Variante".
- **Applying the matches automatically.** Faster, but a wrong match would replace or drop a track
  with no chance to catch it.
- **Replacing a changed item by delete and insert.** Simpler storage, but it breaks tag and share
  links and loses the activity override.
