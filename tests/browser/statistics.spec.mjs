// US-77: the statistics screen, driven as the owner drives it (ADR-0012's
// browser layer).
//
// Scope rule, from the 2026-08-26b amendment: only what `document::eval`
// draws and what a real control change does. The figures, the tables and
// what the screen shows for a given view are asserted in `crates/ui-dioxus`
// (`stats`), without a browser.
import { expect, signIn, test } from "./session.mjs";
import { ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// A chart's series labels, as uPlot holds them: the canvas says nothing.
const series = (page, container) =>
  page.evaluate(
    (container) => window.tripArchiveWidgets[container].series.map((series) => series.label),
    container,
  );

test("the menu opens the statistics with every year's running total drawn (US-77)", async ({
  page,
  request,
}) => {
  await ownTrip(request, "Counted Walk", "hiking");
  await page.goto("/app/");
  await page.locator("#app-menu").getByText("Statistics", { exact: true }).click();

  await expect(page).toHaveURL(/\/app\/stats/);
  await expect(page.locator("#stats-totals")).toBeVisible();
  await expect(page.locator("#stats-running canvas")).toBeVisible();
  // The sample walk is from June 2024: that year has a line of its own.
  expect(await series(page, "stats-running")).toContain("2024");
  // With all activities, a table and no bars.
  await expect(page.locator("#stats-bars")).toHaveCount(0);
});

/// Tick or untick one activity in the picker, opening it first if needed.
async function pick(page, activity) {
  if ((await page.locator("#stats-activity").getAttribute("aria-expanded")) !== "true") {
    await page.locator("#stats-activity").click();
  }
  await page.locator(`#stats-activity-list input[value="${activity}"]`).click();
}

test("choosing activities and a measure redraws the bars and keeps the view in the URL (US-77)", async ({
  page,
  request,
}) => {
  await ownTrip(request, "Counted Paddle", "kayaking");
  await ownTrip(request, "Counted Hike", "hiking");
  await page.goto("/app/stats");

  await pick(page, "kayaking");
  await expect(page).toHaveURL(/activity=kayaking/);
  // The open list shows what is now chosen.
  const box = (value) => page.locator(`#stats-activity-list input[value="${value}"]`);
  await expect(box("kayaking")).toBeChecked();
  await expect(page.locator("#stats-activity-list input").first()).not.toBeChecked();
  await expect(page.locator("#stats-bars canvas")).toBeVisible();
  expect((await series(page, "stats-bars"))[1]).toBe("Distance (km)");

  // A second activity is compared in the table, not drawn as bars.
  await pick(page, "hiking");
  await expect(page).toHaveURL(/activity=hiking,kayaking/);
  await expect(page.locator("#stats-bars")).toHaveCount(0);
  await expect(page.locator("#stats-activity")).toHaveText("Hiking, Kayaking");
  await pick(page, "hiking");
  await expect(page.locator("#stats-bars canvas")).toBeVisible();

  // Escape closes the list; so does a click outside it.
  await page.keyboard.press("Escape");
  await expect(page.locator("#stats-activity-list")).toHaveCount(0);
  await page.locator("#stats-activity").click();
  await page.mouse.click(5, 300);
  await expect(page.locator("#stats-activity-list")).toHaveCount(0);

  await page.locator("#stats-measure").selectOption("moving_time");
  await expect(page).toHaveURL(/measure=moving_time/);
  await expect.poll(async () => (await series(page, "stats-bars"))[1]).toBe("Moving time (h)");

  await page.locator("#stats-period").selectOption("2024");
  await expect(page).toHaveURL(/year=2024/);
  await expect(page.locator("#stats-totals th", { hasText: "Jun" })).toBeVisible();

  // A reload opens the view as it was left.
  await page.reload();
  await expect(page.locator("#stats-activity")).toHaveText("Kayaking");
  await expect(page.locator("#stats-measure")).toHaveValue("moving_time");
  await expect(page.locator("#stats-period")).toHaveValue("2024");
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("the activity column stays put while the months scroll (US-77)", async ({
    page,
    request,
  }) => {
    await ownTrip(request, "Scrolled Walk", "hiking");
    await page.goto("/app/stats?year=2024");
    const box = page.locator("#stats-totals").locator("xpath=..");
    const header = page.locator("#stats-totals tbody th").first();
    await expect(header).toBeVisible();
    const before = await header.boundingBox();

    await box.evaluate((el) => (el.scrollLeft = el.scrollWidth));

    // The months moved; the row's name did not.
    expect(await box.evaluate((el) => el.scrollLeft)).toBeGreaterThan(0);
    const after = await header.boundingBox();
    expect(after.x).toBeCloseTo(before.x, 0);
  });
});

/// A two-point walk in Oslo on `date` (YYYY-MM-DD).
const walkOn = (date) =>
  Buffer.from(
    '<?xml version="1.0"?><gpx version="1.1" creator="t"><trk><name>Year Walk</name><trkseg>' +
      `<trkpt lat="59.91" lon="10.75"><ele>10</ele><time>${date}T08:00:00Z</time></trkpt>` +
      `<trkpt lat="60.31" lon="10.75"><ele>20</ele><time>${date}T18:00:00Z</time></trkpt>` +
      "</trkseg></trk></gpx>",
  );

test("pointing at a year's line or its legend entry singles it out (US-77)", async ({
  page,
  request,
}) => {
  await ownTrip(request, "Older Walk", "hiking", walkOn("2021-03-01"));
  await ownTrip(request, "Old Walk", "hiking", walkOn("2022-03-01"));
  await page.goto("/app/stats?activity=hiking");
  await expect(page.locator("#stats-running canvas")).toBeVisible();
  const focused = page.locator("#stats-running .u-legend .u-series.u-focused");

  // The pointer onto 2021's line, late in the year where it is flat.
  await page.locator("#stats-running").scrollIntoViewIfNeeded();
  const at = await page.evaluate(() => {
    const chart = window.tripArchiveWidgets["stats-running"];
    const index = chart.series.findIndex((series) => series.label === "2021");
    const over = chart.root.querySelector(".u-over").getBoundingClientRect();
    return {
      x: over.left + chart.valToPos(300, "x"),
      y: over.top + chart.valToPos(chart.data[index][300], "y"),
    };
  });
  await page.mouse.move(at.x, at.y);
  await expect(focused).toHaveCount(1);
  await expect(focused).toContainText("2021");

  // Its legend entry does the same for another year.
  await page.locator("#stats-running .u-legend .u-series", { hasText: "2022" }).hover();
  await expect(focused).toContainText("2022");

  // Each other year has a colour of its own.
  const strokes = await page.evaluate(() => {
    const chart = window.tripArchiveWidgets["stats-running"];
    return chart.series
      .filter((series) => series.label === "2021" || series.label === "2022")
      .map((series) => series._stroke);
  });
  expect(new Set(strokes).size).toBe(2);
});
