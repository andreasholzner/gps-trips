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

/// The chart's value axis label.
const axis = (page) => page.evaluate(() => window.tripArchiveWidgets["stats-plot"].axes[1].label);

/// The totals table, unfolded from under the chart.
async function unfold(page) {
  await page.locator(".stats-table-details summary").click();
  await expect(page.locator("#stats-totals")).toBeVisible();
}

test("the menu opens the statistics with the totals drawn over a folded table (US-77)", async ({
  page,
  request,
}) => {
  await ownTrip(request, "Counted Walk", "hiking");
  await page.goto("/app/");
  await page.locator("#app-menu").getByText("Statistics", { exact: true }).click();

  await expect(page).toHaveURL(/\/app\/stats/);
  await expect(page.locator("#stats-plot canvas")).toBeVisible();
  expect(await series(page, "stats-plot")).toContain("Hiking");
  // The table is folded away until asked for.
  await expect(page.locator("#stats-totals")).toBeHidden();
  await unfold(page);
  // An activity's icon is named on a pointer's hover.
  await expect(page.locator("#stats-totals tbody th .activity-icon").first()).toHaveAttribute(
    "title",
    "Hiking",
  );
});

/// Tick or untick one activity in the picker, opening it first if needed.
async function pick(page, activity) {
  if ((await page.locator("#stats-activity").getAttribute("aria-expanded")) !== "true") {
    await page.locator("#stats-activity").click();
  }
  await page.locator(`#stats-activity-list input[value="${activity}"]`).click();
}

test("choosing activities and a measure redraws the chart and keeps the view in the URL (US-77)", async ({
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
  await expect(page.locator("#stats-plot canvas")).toBeVisible();
  expect(await series(page, "stats-plot")).toEqual(["Year", "Kayaking"]);
  expect(await axis(page)).toBe("Distance (km)");

  // A second activity is stacked on the first.
  await pick(page, "hiking");
  await expect(page).toHaveURL(/activity=hiking,kayaking/);
  await expect(page.locator("#stats-activity")).toHaveText("Hiking, Kayaking");
  await expect.poll(() => series(page, "stats-plot")).toEqual(["Year", "Kayaking", "Hiking"]);
  await pick(page, "hiking");
  await expect.poll(() => series(page, "stats-plot")).toEqual(["Year", "Kayaking"]);

  // Escape closes the list; so does a click outside it.
  await page.keyboard.press("Escape");
  await expect(page.locator("#stats-activity-list")).toHaveCount(0);
  await page.locator("#stats-activity").click();
  await page.mouse.click(5, 300);
  await expect(page.locator("#stats-activity-list")).toHaveCount(0);

  await page.locator("#stats-measure").selectOption("moving_time");
  await expect(page).toHaveURL(/measure=moving_time/);
  await expect.poll(() => axis(page)).toBe("Moving time (h)");

  await page.locator("#stats-period").selectOption("2024");
  await expect(page).toHaveURL(/year=2024/);
  await unfold(page);
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
    await unfold(page);
    const box = page.locator("#stats-totals").locator("xpath=..");
    const header = page.locator("#stats-totals tbody th").first();
    await expect(header).toBeVisible();
    const before = await header.boundingBox();

    await box.evaluate((el) => (el.scrollLeft = el.scrollWidth));

    // The months moved; the row's icon did not.
    expect(await box.evaluate((el) => el.scrollLeft)).toBeGreaterThan(0);
    const after = await header.boundingBox();
    expect(after.x).toBeCloseTo(before.x, 0);
  });
});

test("a ratio is drawn as a line per activity and one for them together (US-80)", async ({
  page,
  request,
}) => {
  await ownTrip(request, "Speed Walk", "hiking");
  await ownTrip(request, "Speed Paddle", "kayaking");
  await page.goto("/app/stats?measure=average_speed");

  await expect(page.locator("#stats-plot canvas")).toBeVisible();
  const labels = await series(page, "stats-plot");
  expect(labels).toContain("Hiking");
  expect(labels).toContain("Kayaking");
  expect(labels.at(-1)).toBe("All activities");
});
