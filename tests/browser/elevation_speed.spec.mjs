// US-79: the elevation profile also shows how fast the owner went, and the
// readout how fast and how steep it was where the cursor is (ADR-0012's
// browser layer).
//
// Scope rule, from the 2026-08-26b amendment: only what `document::eval`
// draws and what a real pointer does. The series themselves, the readout's
// markup and its dashes are asserted in `crates/ui-dioxus`.
import { expect, signIn, test } from "./session.mjs";
import { ownTrips, UNTIMED_GPX } from "./trips.mjs";

const ownTrip = ownTrips(test);

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// The chart's series and which side each axis is drawn on, as uPlot holds
/// them: the canvas says nothing about either.
const drawn = (page) =>
  page.evaluate(() => {
    const chart = window.tripArchiveWidgets.elevation;
    return {
      series: chart.series.map((series) => series.label),
      scales: chart.series.map((series) => series.scale),
      sides: chart.axes.map((axis) => axis.side),
    };
  });

/// The pointer onto the middle of the chart, in view first: `page.mouse`
/// works in viewport coordinates and does not scroll.
async function hoverTheMiddle(page) {
  await page.locator("#elevation").scrollIntoViewIfNeeded();
  const chart = await page.locator("#elevation").boundingBox();
  await page.mouse.move(chart.x + chart.width / 2, chart.y + chart.height / 2);
}

test("a recorded trip's profile draws its speed against a right axis (US-79)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Paced Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#elevation canvas")).toBeVisible();

  // Speed is a series of its own on a scale of its own, drawn against an
  // axis on the right (uPlot's side 1); elevation keeps its scale and its
  // axis on the left (side 3), over the one distance axis both share — which
  // is what makes a drag zoom both.
  const chart = await drawn(page);
  expect(chart.series).toEqual(["Distance (km)", "Elevation (m)", "Speed (km/h)"]);
  expect(chart.scales).toEqual(["x", "m", "kmh"]);
  expect(chart.sides).toEqual([2, 3, 1]);

  await hoverTheMiddle(page);
  await expect(page.locator("#readout-speed")).toHaveText(/^\d+\.\d km\/h$/);
  await expect(page.locator("#readout-incline")).toHaveText(/^([+−]\d+|0) %$/);
});

test("a planned trip's profile is the elevation alone (US-79)", async ({ page, request }) => {
  const id = await ownTrip(request, "Planned Trip", undefined, UNTIMED_GPX);
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#elevation canvas")).toBeVisible();

  const chart = await drawn(page);
  expect(chart.series).toEqual(["Distance (km)", "Elevation (m)"]);
  expect(chart.sides).not.toContain(1);

  // No speed to read out, ever — but the incline is the track's own.
  await hoverTheMiddle(page);
  await expect(page.locator("#readout-incline")).toHaveText(/^([+−]\d+|0) %$/);
  await expect(page.locator("#readout-speed")).toHaveCount(0);
});

/// The left edge of each field in the readout, in page pixels.
const fieldEdges = (page) =>
  page
    .locator("#chart-readout [id^=readout-]")
    .evaluateAll((fields) => fields.map((field) => Math.round(field.getBoundingClientRect().left)));

// Layout: a value with more digits than the last, or a dash where a value
// was, must not push the fields after it along — the readout would twitch on
// every move across the chart. Only a browser lays text out.
test("the readout's fields stay put as the cursor moves (US-79)", async ({ page, request }) => {
  const id = await ownTrip(request, "Steady Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#elevation canvas")).toBeVisible();
  await page.locator("#elevation").scrollIntoViewIfNeeded();
  // The plotting area itself, not the axes around it.
  const chart = await page.locator("#elevation .u-over").boundingBox();

  // Nothing hovered: every field a dash.
  await expect(page.locator("#readout-distance")).toHaveText("—");
  const resting = await fieldEdges(page);
  expect(resting).toHaveLength(5);

  for (const along of [0.02, 0.5, 0.98]) {
    await page.mouse.move(chart.x + chart.width * along, chart.y + chart.height / 2);
    await expect(page.locator("#readout-distance")).not.toHaveText("—");
    expect(await fieldEdges(page), `hovered at ${along}`).toEqual(resting);
  }

  // The widest each field plausibly reads still fits its slot: written into
  // the fields directly, since no one fixture reaches every extreme.
  const widest = await page.locator("#chart-readout [id^=readout-]").evaluateAll((fields) => {
    const values = ["1234.56 km", "8848 m", "123.4 km/h", "−100 %"];
    values.forEach((value, i) => (fields[i].textContent = value));
    return fields.map((field) => Math.round(field.getBoundingClientRect().left));
  });
  expect(widest).toEqual(resting);
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  // Layout: the readout wraps on a narrow screen, and a separator must wrap
  // with the value it introduces rather than start a line on its own.
  test("the readout wraps between readings, never inside one (US-79)", async ({
    page,
    request,
  }) => {
    const id = await ownTrip(request, "Narrow Trip");
    await page.goto(`/app/trips/${id}`);
    await expect(page.locator("#elevation canvas")).toBeVisible();

    const readout = page.locator("#chart-readout");
    const lines = await readout.evaluate((p) =>
      Math.round(p.offsetHeight / parseFloat(getComputedStyle(p).lineHeight)),
    );
    expect(lines, "narrow enough to wrap").toBeGreaterThan(1);
    const pieces = await page
      .locator("#chart-readout .reading")
      .evaluateAll((readings) => readings.map((reading) => reading.getClientRects().length));
    expect(pieces).toHaveLength(5);
    expect(pieces.every((count) => count === 1), `${pieces}`).toBe(true);
  });
});
