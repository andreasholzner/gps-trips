// Moving the region on the trip-list map by touch (US-92).
//
// A finger's drag is a real user event only a touchscreen context sends, and
// the map is drawn by JS through `document::eval` — both of this layer's
// exemptions at once (ADR-0012). The mouse pans stay in `trip_list.spec.mjs`
// as the regression for the same code path.
import { expect, signIn, test } from "./session.mjs";
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

  test("with Filter to map on, a finger drag pans the map, the region follows, and the page stays put", async ({
    page,
    request,
  }) => {
    await ownTrip(request, "Touched Region");
    await page.goto("/app/");
    await expect(page.locator(TRIP).first()).toBeVisible();
    await page.locator("#region-follow").tap();
    await expect(page).toHaveURL(/[?&]bbox=/);
    const [west] = bboxOf(page);

    // Playwright's touchscreen can tap but not drag, so the finger is
    // dispatched as real touch input. Read where the map is now: tapping
    // the checkbox below it may have scrolled it.
    const touch = await page.context().newCDPSession(page);
    const map = await page.locator("#region-map").boundingBox();
    const at = (fx) => ({ x: map.x + map.width * fx, y: map.y + map.height * 0.5 });
    const scrolledTo = await page.evaluate(() => window.scrollY);
    await touch.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [at(0.1)] });
    for (const f of [0.25, 0.5, 0.75, 1]) {
      await touch.send("Input.dispatchTouchEvent", {
        type: "touchMove",
        touchPoints: [at(0.1 + 0.8 * f)],
      });
    }
    // The drag is the map's, not the page's.
    expect(await page.evaluate(() => window.scrollY)).toBe(scrolledTo);
    await touch.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });

    // Dragged east, the map looks further west.
    await expect.poll(() => bboxOf(page)[0]).toBeLessThan(west);
  });
});
