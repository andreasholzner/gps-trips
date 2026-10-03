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

test("choosing an activity and a measure redraws the bars and keeps the view in the URL (US-77)", async ({
  page,
  request,
}) => {
  await ownTrip(request, "Counted Paddle", "kayaking");
  await page.goto("/app/stats");

  await page.locator("#stats-activity").selectOption("kayaking");
  await expect(page).toHaveURL(/activity=kayaking/);
  await expect(page.locator("#stats-bars canvas")).toBeVisible();
  expect((await series(page, "stats-bars"))[1]).toBe("Distance (km)");

  await page.locator("#stats-measure").selectOption("moving_time");
  await expect(page).toHaveURL(/measure=moving_time/);
  await expect.poll(async () => (await series(page, "stats-bars"))[1]).toBe("Moving time (h)");

  await page.locator("#stats-period").selectOption("2024");
  await expect(page).toHaveURL(/year=2024/);
  await expect(page.locator("#stats-totals th", { hasText: "Jun" })).toBeVisible();

  // A reload opens the view as it was left.
  await page.reload();
  await expect(page.locator("#stats-activity")).toHaveValue("kayaking");
  await expect(page.locator("#stats-measure")).toHaveValue("moving_time");
  await expect(page.locator("#stats-period")).toHaveValue("2024");
});
