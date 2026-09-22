// US-62 driven as the owner drives it: the trip's page at a glance, and a
// photo looked at properly (ADR-0012's browser layer).
//
// Scope rule, from the 2026-08-26b amendment: only what the host-target tests
// structurally cannot reach — real user events, what `document::eval` draws,
// and layout, which only a browser computes. What the screen renders is
// asserted in `crates/ui-dioxus`.
import { expect, signIn, test } from "./session.mjs";
import { ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// Where each stats pair sits, as `[left, top]`.
const pairPositions = (page) =>
  page.locator("dl.stats > div").evaluateAll((pairs) =>
    pairs.map((pair) => {
      const box = pair.getBoundingClientRect();
      return [Math.round(box.left), Math.round(box.top)];
    }),
  );

// Layout: five pairs in one row from US-60's breakpoint up.
test("on a wide screen the numbers sit in one row (US-62)", async ({ page, request }) => {
  const id = await ownTrip(request, "Wide Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#trip-activity")).toBeVisible();

  const pairs = await pairPositions(page);
  expect(pairs).toHaveLength(5);
  expect(new Set(pairs.map(([, top]) => top)).size, "one row").toBe(1);
  // Label over value, not beside it.
  const [label, value] = await page
    .locator("dl.stats > div")
    .first()
    .evaluate((pair) => [...pair.children].map((el) => el.getBoundingClientRect().top));
  expect(value).toBeGreaterThan(label);
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("the numbers are a two-column grid (US-62)", async ({ page, request }) => {
    const id = await ownTrip(request, "Narrow Trip");
    await page.goto(`/app/trips/${id}`);
    await expect(page.locator("#trip-activity")).toBeVisible();

    const [first, second, third] = await pairPositions(page);
    expect(second[1], "the first two share a row").toBe(first[1]);
    expect(third[1], "the third starts the next").toBeGreaterThan(first[1]);
    expect(third[0], "in the first column").toBe(first[0]);
  });
});

// A real pointer over the chart; the time comes back through Rust.
test("hovering the profile reads out the time, with its offset (US-62)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Timed Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#elevation canvas")).toBeVisible();
  await expect(page.locator("#readout-time")).toHaveText("—");

  await page.locator("#elevation").scrollIntoViewIfNeeded();
  const chart = await page.locator("#elevation").boundingBox();
  await page.mouse.move(chart.x + chart.width / 2, chart.y + chart.height / 2);

  // SAMPLE_GPX is a June morning in Oslo, 08:00–09:00 UTC: 10:00–11:00 there,
  // and never across midnight, so a clock time without a date.
  await expect(page.locator("#readout-time")).toHaveText(/^1[01]:\d\d \(\+02:00\)$/);
});

// The edit form over the screen: a real click, a key, and a page that holds
// still behind it.
test("editing opens over the screen and Escape closes it (US-62)", async ({ page, request }) => {
  const id = await ownTrip(request, "Overlaid Trip");
  await page.goto(`/app/trips/${id}`);

  await page.getByRole("button", { name: "Edit name / activity" }).click();
  const dialog = page.getByRole("dialog", { name: "Edit trip" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel("Name")).toBeVisible();
  await expect(page.locator("body")).toHaveClass(/overlay-open/);
  expect(await page.evaluate(() => getComputedStyle(document.body).overflow)).toBe("hidden");

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.locator("body")).not.toHaveClass(/overlay-open/);

  // And its own Cancel does the same.
  await page.getByRole("button", { name: "Edit name / activity" }).click();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toHaveCount(0);
});
