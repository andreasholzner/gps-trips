# ADR-0030 — Links between what was done and what was planned are proposed from coverage, and the owner decides

## Status

Accepted

## Context

Linking a guide book's stages and the owner's plans to recorded trips by hand does not scale
([US-103](../requirements.md)). Recorded and planned lines never coincide: a recording strays a few
metres from a path, a plan along paths is drawn on them, and a plan off paths is a loose string of
points joined by straight lines the trip strays far from between them. A trip may follow a whole
stage within a longer day, or one day of a multi-day plan. The direction of travel says nothing
about whether a plan was followed. [ADR-0029](./0029-collection-re-import-matching.md) matches a
file against a collection to find the *same* item; this is about one thing having *followed*
another. The server is stopped between uses
([ADR-0023](./0023-managed-scale-to-zero-hosting.md)), so work started by a request cannot count on
running after it.

## Decision

**The server proposes a link when the trip covers enough of the plan, or ran enough on it; the
owner confirms or dismisses each proposal.**

- **Coverage is measured both ways:** *plan covered*, the share of the plan within a tolerance of
  the trip, and *trip on plan*, the share of the trip within a tolerance of the plan. A pair is
  proposed when the plan is mostly covered, or when the trip ran mostly on the plan over a minimum
  length. Short stretches near the other line do not count.
- **A plan is judged by its line and by its points, and either suffices.** The points — the plan's
  own, not its GPX waypoints — have a wider tolerance, so a plan off paths is recognised without
  being told apart from one along paths.
- **A waypoint is passed when any part of the line comes within a distance of it.**
- **The direction of travel is ignored.**
- **A proposal is a link marked as proposed.** It is kept as links are, outlasts a restart, and
  counts as a link nowhere until confirmed. A dismissed proposal is not remembered.
- **Matching runs within the request that brings an item in, or that the owner sends** — never as
  a background job.
- The tolerances, shares and minimum lengths are configuration, not part of this decision.

## Consequences

- Guide-book stages, plans and multi-day plans are proposed without linking by hand, and nothing
  becomes a link without the owner.
- Coverage explains a proposal in figures the owner can read at a glance, without an invented
  confidence.
- A trip on a parallel path near a plan can be proposed wrongly, more so under the looser points
  rule; the owner dismisses it.
- A dismissed pair comes back when the owner starts a check again or a collection item is replaced.
- Matching adds to the time an import or sync takes; it must stay fast enough for a request, so
  candidates are narrowed by their extent before any line is measured.
- Trips already in the archive are matched only when the owner starts a check from an item's page.

## Alternatives considered

- **Linking automatically above a threshold.** Faster, but a wrong link would silently change
  counts and marks.
- **Plan covered alone.** Misses every day of a multi-day plan.
- **Line coverage alone.** Misses plans drawn off paths, whose straight lines the trip does not
  follow.
- **Flagging plans as off paths, at import or by the owner.** Nothing in a GPX file says so
  reliably, and it adds a choice for the owner.
- **Proposals apart from links, or only in memory.** A separate store repeats how links are kept
  and removed; memory loses proposals when the server stops.
- **A background job or a search over the whole archive.** Does not fit a server that stops
  between uses, and checking one item from its page is enough.
