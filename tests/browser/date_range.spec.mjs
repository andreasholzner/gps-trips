// US-110: the trip list's date slider, driven as the owner drives it
// (ADR-0012's browser layer).
//
// Scope rule, from the 2026-08-26b amendment: only real user events — key
// presses and drags on the handles, and a handle pushed back by the page's
// script. What the months hold, where a date puts a handle and what a handle
// writes are asserted in `crates/ui-dioxus` (`date_range`), without a browser.
import { expect, signIn, test } from "./session.mjs";
import { chooseActivity, ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

/// The trip `beforeAll` imports, by its name.
let sliderWalk;

test.beforeAll(async ({ request }) => {
  // A trip from Jun 2024, so the slider spans more than the current month
  // whatever else the shared archive holds.
  const id = await ownTrip(request, "Slider Walk", "hiking");
  sliderWalk = (await (await request.get(`/api/trips/${id}`)).json()).name;
});

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
  await page.goto("/app/");
  await page.getByText("More filters").click();
});

const earliest = (page) => page.getByRole("slider", { name: "Earliest month" });
const latest = (page) => page.getByRole("slider", { name: "Latest month" });

test("a handle moves a month per key press, and at an end sets no limit", async ({ page }) => {
  await expect(page.getByText("Any date")).toBeVisible();

  await earliest(page).focus();
  await page.keyboard.press("End");
  const month = new Date().toISOString().slice(0, 7);
  await expect(page).toHaveURL(new RegExp(`[?&]from=${month}-01(&|$)`));

  await page.keyboard.press("ArrowLeft");
  await expect(page).not.toHaveURL(new RegExp(`from=${month}-01`));
  await expect(page).toHaveURL(/[?&]from=\d{4}-\d{2}-01(&|$)/);

  await page.keyboard.press("Home");
  await expect(page).not.toHaveURL(/from=/);
  await expect(page.getByText("Any date")).toBeVisible();
});

test("a handle dragged past the other stays with it", async ({ page }) => {
  await latest(page).focus();
  await page.keyboard.press("Home");
  await expect(page).toHaveURL(/[?&]to=\d{4}-\d{2}-\d{2}/);

  await earliest(page).focus();
  await page.keyboard.press("End");

  await expect(earliest(page)).toHaveValue("0");
  await expect(latest(page)).toHaveValue("0");
});

test("both handles can be dragged, and the bars between them are highlighted", async ({ page }) => {
  const box = await page.locator(".date-slider").boundingBox();
  const middle = box.y + box.height / 2;
  const drag = async (fromX, toX) => {
    await page.mouse.move(fromX, middle);
    await page.mouse.down();
    await page.mouse.move(toX, middle, { steps: 8 });
    await page.mouse.up();
  };

  // The right handle sits on the right end, above the left one's input.
  // While it is held the list stays as it is; letting go narrows it.
  await page.mouse.move(box.x + box.width - 10, middle);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.75, middle, { steps: 8 });
  await expect(page.locator(".date-range output")).not.toHaveText("Any date");
  await expect(page).not.toHaveURL(/to=/);
  await page.mouse.up();
  await expect(page).toHaveURL(/[?&]to=\d{4}-\d{2}-\d{2}/);

  // The left handle is reached through the right one's input.
  await drag(box.x + 10, box.x + box.width * 0.25);
  await expect(page).toHaveURL(/[?&]from=\d{4}-\d{2}-01/);

  await expect(page.locator(".date-bars .bar.in").first()).toBeAttached();
  await expect(page.locator(".date-bars .bar:not(.in)").first()).toBeAttached();
});

// SelectionBar's bulk change is a real click: afterwards the bars count what
// the other filters match now, not what they matched before it.
test("a bulk change of activity is reflected in the bars while a date is set", async ({ page }) => {
  const matching = page.locator(".date-bars .match:not([height='0'])");
  await page.goto(`/app/?activity=hiking&q=${encodeURIComponent(sliderWalk)}&to=2024-12-31`);
  await expect(page.locator("table tbody tr")).toHaveCount(1);
  await page.getByText("More filters").click();
  await expect(matching).toHaveCount(1);

  await page.locator("table thead").getByRole("checkbox").check();
  await page.getByRole("button", { name: "Set activity…" }).click();
  await chooseActivity(page, "bulk-activity", "cycling");
  await page.getByRole("button", { name: "Set for 1 selected" }).click();
  await page.getByRole("button", { name: "Change", exact: true }).click();

  await expect(page.getByText("No trips match your filters.")).toBeVisible();
  await expect(matching).toHaveCount(0);
});
