// US-81: the climbs marked in the elevation profile (ADR-0012's browser
// layer).
//
// Scope rule, from the 2026-08-26b amendment: only what `document::eval`
// draws and what a real pointer does. Which samples lie on a climb, the
// climbs list and the climbing rate are asserted in `crates/ui-dioxus`.
import { expect, signIn, test } from "./session.mjs";
import { HILL_GPX, ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// The chart's series, and how many of the climbs series' samples are
/// filled — as uPlot holds them: the canvas says neither.
const drawn = (page) =>
  page.evaluate(() => {
    const chart = window.tripArchiveWidgets.elevation;
    const climbs = chart.series.findIndex((series) => series.label === "Climbs");
    return {
      series: chart.series.map((series) => series.label),
      samples: chart.data[0].length,
      filled: climbs < 0 ? 0 : chart.data[climbs].filter((value) => value !== null).length,
    };
  });

test("a climb is filled below the elevation line, and the speed and readout stay (US-81)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Hill Walk", "hiking", HILL_GPX);
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#climbs")).toBeVisible();
  await expect(page.locator("#elevation canvas")).toBeVisible();

  const chart = await drawn(page);
  // The fill first, so the line is drawn over it.
  expect(chart.series).toEqual(["Distance (km)", "Climbs", "Elevation (m)", "Speed (km/h)"]);
  // The hill, and not the flats either side of it.
  expect(chart.filled).toBeGreaterThan(chart.samples / 2);
  expect(chart.filled).toBeLessThan(chart.samples);

  await page.locator("#elevation").scrollIntoViewIfNeeded();
  const box = await page.locator("#elevation").boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.locator("#readout-speed")).toHaveText(/^\d+\.\d km\/h$/);
});
