# Spike — can `document::eval` carry a sustained two-way channel?

**Date:** 2026-08-29 · **Branch:** `spike-eval-two-way` (throwaway) ·
**Verdict: yes — [ADR-0025](./adr/0025-js-widget-interop-via-eval.md) needs no amendment.**

[US-52](./requirements.md) filters the trip list by dragging a rectangle on a map, which means
the map must report *back* into Rust state. ADR-0025 accepted `document::eval` for "draw this"
widgets and named this exact case as the trigger to re-examine itself:

> `eval`'s send-and-forget shape suits "draw this"; a map that must continuously report clicks,
> drags and viewport changes back into Rust state would strain it.

So US-52 starts here rather than with implementation. If the mechanism could not carry it, the
next step would have been an ADR-0025 amendment, proposed and approved before any code. It can,
so implementation proceeds under the ADR unchanged.

## What was built

A `RegionMap` component holding **one** long-lived `Eval` in a `use_future`, calling `recv()` in
a loop; a vendored Leaflet drawing a rectangle and calling `dioxus.send()` once per finished
drag. It was mounted inside the real trip-list screen, with the resulting `bbox` appended to the
list query — so every drag triggers a re-query and a re-render, which is the pressure the
channel actually has to survive. Verified in headless Chromium against the built bundle
(`tests/browser/spike_region.spec.mjs`).

OSM tiles were deliberately left out: coordinate maths needs no tile layer, and a spike should
not hammer someone else's tile server on every run.

## Findings

**1. The channel is sustained, not one-shot.** One `Eval` delivered message after message.
`Eval::recv()` takes `&mut self` by design, and a `use_future` holds the handle for the
component's whole life. Two drags in a row both arrived; a third arrived after the screen had
been re-rendered many times over.

**2. It survives re-renders, including its own feedback.** `use_future` runs once per component
instance, so re-rendering does not restart the eval or reload the script. The drag → `bbox` →
re-query → re-render path — the one most likely to bite, because the component re-renders as a
*consequence* of the message it just received — completed cleanly, repeatedly.

**3. Nothing is dropped under a burst.** Five `dioxus.send()` calls in a single tick, with Rust
demonstrably not parked in `recv()`, all arrived and in order. The channel queues; a fast
sequence of drags will not lose events.

**4. The container discipline holds.** Dioxus renders the container empty, Leaflet owns it
afterwards, and the script's own re-init guard never fired. Exactly one `.leaflet-container`
existed after all the drags and re-renders, and no console errors appeared.

**5. The loop is real, end to end.** A rectangle away from the seeded trip reduced the list to
zero rows; a rectangle covering it brought it back. Drag → Rust state → JSON API → re-render
works as a whole, not just as a message arriving.

## What this cost, and what to carry into the implementation

- **`bbox` must be a field of `Filters`, not a signal beside it.** The spike bolted it on
  outside, and the screen then showed "No trips yet" instead of "No trips match your filters" —
  `Filters::any_set()` could not see it. US-14 requires the *filtered* empty state for a region
  containing no trips, so this is a real constraint, not a detail.
- **The script needs two guards, not one.** ADR-0025 already requires waiting for the library's
  global; `use_future` can also run before the container it draws into is mounted, so the script
  must wait for the element too.
- **`asset!` needs Leaflet inside the crate.** A second copy alongside `public/vendor`, which
  ADR-0025 already accepts and which resolves when the PoC UI retires.
- **Browser specs share one archive.** Adding a spec that seeds its own trips broke the US-41
  specs' row counts; each passes alone. Whatever US-52 adds must either share the seeding or
  stop asserting on absolute totals.
- Raw Rust string literals need `r##"…"##` for scripts containing `"#` — a CSS colour such as
  `"#3388ff"` silently terminates an `r#"…"#` literal.

## Not covered

Android was not tested: the app needs a physical device, and it is US-16's own step. The
mechanism is the same webview on both platforms, which is why ADR-0024 chose `eval` at all, but
that inference is not evidence. Viewport/pan/zoom reporting was not spiked either — US-52 needs
only the rectangle; if a later story wants continuous viewport sync, that is a fresh question.

## Cleanup

Everything here is throwaway: the branch holds `crates/ui-dioxus/src/spike_region.rs`, its
mounting inside `list.rs`, the Leaflet assets, and `tests/browser/spike_region.spec.mjs`. The
implementation starts from `master` with these findings, not from this code.
