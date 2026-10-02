// Selecting a region on the trip-list map by touch (US-65).
//
// A finger's drag is a real user event only a touchscreen context sends, and
// the map is drawn by JS through `document::eval` — both of this layer's
// exemptions at once (ADR-0012). The mouse drag stays in `trip_list.spec.mjs`
// as the regression for the same code path.
import { expect, signIn, test } from "./session.mjs";
import { tileZooms } from "./map.mjs";
import { ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// The corners in the URL's `bbox`, or `null` when there is none.
function bboxOf(page) {
  const param = new URL(page.url()).searchParams.get("bbox");
  return param === null ? null : param.split(",").map(Number);
}

/// A trip on the map: a heat mark, or zoomed in as far as the fixture's
/// short walk fits, its line (US-73).
const TRIP = "#region-map :is(.heat-mark, .trip-line)";

test.describe("with a touchscreen", () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });

  let touch;

  test.beforeEach(async ({ page, request }) => {
    await ownTrip(request, "Touched Region");
    await page.goto("/app/");
    await expect(page.locator(TRIP).first()).toBeVisible();
    // Playwright's touchscreen can tap but not drag, so fingers are
    // dispatched as real touch input.
    touch = await page.context().newCDPSession(page);
  });

  /// Put fingers down at `from`, move them to `to`, and lift them — each
  /// finger a point given as fractions of the map's width and height.
  /// `whileDown` runs before the fingers lift.
  async function drag(page, from, to, whileDown = async () => {}) {
    // Read where the map is now: tapping the button below it scrolls it.
    const map = await page.locator("#region-map").boundingBox();
    const at = ([fx, fy]) => ({ x: map.x + map.width * fx, y: map.y + map.height * fy });
    await touch.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: from.map(at) });
    for (const f of [0.25, 0.5, 0.75, 1]) {
      await touch.send("Input.dispatchTouchEvent", {
        type: "touchMove",
        touchPoints: from.map(([fx, fy], i) =>
          at([fx + (to[i][0] - fx) * f, fy + (to[i][1] - fy) * f]),
        ),
      });
    }
    await whileDown();
    await touch.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  }

  test("a finger drag draws the region, and the page stays put", async ({ page }) => {
    const select = page.locator("#region-select");
    await select.tap();
    await expect(select).toHaveAttribute("aria-pressed", "true");
    await expect(select).toHaveText("Cancel selection");
    const scrolledTo = await page.evaluate(() => window.scrollY);

    await drag(page, [[0.1, 0.3]], [[0.4, 0.7]], async () => {
      // The drag is the map's, not the page's. Asked before lifting: once
      // the region applies, the list below shrinks and the page with it.
      expect(await page.evaluate(() => window.scrollY)).toBe(scrolledTo);
    });

    await expect(page).toHaveURL(/[?&]bbox=/);
    const [west, south, east, north] = bboxOf(page);
    expect(west).toBeLessThan(east);
    expect(south).toBeLessThan(north);
    // Done: an ordinary drag pans again.
    await expect(select).toHaveAttribute("aria-pressed", "false");
    await expect(select).toHaveText("Select area");
  });

  test("a tap is not a region, and the map stays armed", async ({ page }) => {
    const select = page.locator("#region-select");
    await select.tap();

    await drag(page, [[0.5, 0.5]], [[0.5, 0.5]]);
    await expect(select).toHaveAttribute("aria-pressed", "true");

    // Still armed, so the next drag draws — and what reaches the URL is that
    // rectangle, not a zero-size one from the tap.
    await drag(page, [[0.1, 0.3]], [[0.4, 0.7]]);
    await expect(page).toHaveURL(/[?&]bbox=/);
    const [west, south, east, north] = bboxOf(page);
    expect(west).toBeLessThan(east);
    expect(south).toBeLessThan(north);
  });

  test("while armed, a tap on a trip's line opens nothing (US-73)", async ({ page }) => {
    // The fixture's walk fits zoomed in past the threshold, so it is a line.
    const line = page.locator("#region-map .trip-line").first();
    await expect(line).toBeVisible();
    const select = page.locator("#region-select");
    await select.tap();
    const map = await page.locator("#region-map").boundingBox();
    const at = await line.evaluate((path) => {
      const p = path
        .getPointAtLength(path.getTotalLength() / 2)
        .matrixTransform(path.getScreenCTM());
      return { x: p.x, y: p.y };
    });

    await drag(
      page,
      [[(at.x - map.x) / map.width, (at.y - map.y) / map.height]],
      [[(at.x - map.x) / map.width, (at.y - map.y) / map.height]],
    );

    // Nothing to wait *for* when nothing should happen: give opening the
    // trip the time it would take, then look.
    await page.waitForTimeout(300);
    await expect(page).toHaveURL(/\/app\/(\?|$)/);
    await expect(select).toHaveAttribute("aria-pressed", "true");
  });

  test("tapping the button again backs out, and a drag pans", async ({ page }) => {
    const select = page.locator("#region-select");
    await select.tap();
    await select.tap();
    await expect(select).toHaveAttribute("aria-pressed", "false");
    await expect(select).toHaveText("Select area");

    const mark = page.locator(TRIP).first();
    const before = await mark.boundingBox();
    await drag(page, [[0.3, 0.5]], [[0.8, 0.5]]);

    await expect.poll(async () => (await mark.boundingBox())?.x).toBeGreaterThan(before.x + 50);
    expect(bboxOf(page)).toBeNull();
  });

  test("while armed, two fingers neither zoom the map nor the page", async ({ page }) => {
    const zooms = await tileZooms(page, "region-map");
    await page.locator("#region-select").tap();

    // Spread two fingers apart: a pinch-out.
    await drag(
      page,
      [
        [0.45, 0.45],
        [0.55, 0.55],
      ],
      [
        [0.2, 0.2],
        [0.8, 0.8],
      ],
    );

    // Nothing to wait *for* when nothing should happen: give a zoom the time
    // its animation would take, then look.
    await page.waitForTimeout(500);
    expect(await tileZooms(page, "region-map")).toEqual(zooms);
    expect(await page.evaluate(() => window.visualViewport.scale)).toBe(1);
  });
});
