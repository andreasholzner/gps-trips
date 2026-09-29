// The trip-list map zoomed in (US-73): past a zoom threshold it draws the
// trips in view as their tracks rather than heat marks.
//
// Zooming, hovering, clicking a line and dragging from one are real user
// events on a map drawn by JS through `document::eval` — this layer's
// exemptions (ADR-0012). Which trips get a line, and in which color, is
// asserted in `crates/ui-dioxus` (`trip_lines`).
import { expect, signIn, test } from "./session.mjs";
import { zoomBy } from "./map.mjs";
import { ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

const lines = (page) => page.locator("#region-map .trip-line");
const marks = (page) => page.locator("#region-map .heat-mark");

/// The list narrowed by name to `q`, so only this test's trips are on the
/// map: every fixture track is the same walk in Oslo.
const listOf = (q) => `/app/?q=${encodeURIComponent(q)}`;

/// The trip's name, as the import gave it.
const nameOf = async (request, id) => (await (await request.get(`/api/trips/${id}`)).json()).name;

/// A point on the line itself, halfway along it, in viewport coordinates.
const onTheLine = (line) =>
  line.evaluate((path) => {
    const at = path
      .getPointAtLength(path.getTotalLength() / 2)
      .matrixTransform(path.getScreenCTM());
    return { x: at.x, y: at.y };
  });

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test("zoomed in, the trips are lines; zoomed out, marks again", async ({ page, request }) => {
  const id = await ownTrip(request, "Lines Walk", "hiking");
  const name = await nameOf(request, id);
  await page.goto(listOf(name));

  // Fitted to one short walk, the map is zoomed in past the threshold.
  await expect(lines(page)).toHaveCount(1);
  await expect(marks(page)).toHaveCount(0);
  await expect(lines(page)).toHaveAttribute("stroke", "#b2182b");

  // Fitted to a single short walk, it is at 12 — the "Fit to trips" cap.
  // The threshold is 9, so four steps out is the first zoom with marks.
  await zoomBy(page, "region-map", "Zoom out", 4, 12);
  await expect(marks(page)).toHaveCount(1);
  await expect(lines(page)).toHaveCount(0);

  await zoomBy(page, "region-map", "Zoom in", 1, 8);
  await expect(lines(page)).toHaveCount(1);
  await expect(marks(page)).toHaveCount(0);
});

test("a line is named and highlighted on hover, and a click opens its trip", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Hovered Walk", "hiking");
  const name = await nameOf(request, id);
  await page.goto(listOf(name));
  await expect(lines(page)).toHaveCount(1);
  await page.locator("#region-map").scrollIntoViewIfNeeded();

  await expect(lines(page)).toHaveAttribute("stroke-width", "3");
  const at = await onTheLine(lines(page));
  await page.mouse.move(at.x, at.y);
  await expect(page.locator("#region-map .leaflet-tooltip")).toHaveText(name);
  await expect(lines(page)).toHaveAttribute("stroke-width", "6");

  // Off the line, it is drawn as before.
  await page.mouse.move(at.x, at.y - 60);
  await expect(lines(page)).toHaveAttribute("stroke-width", "3");
  await page.mouse.move(at.x, at.y);

  await page.mouse.click(at.x, at.y);
  await expect(page).toHaveURL(new RegExp(`/app/trips/${id}$`));
});

test("a change of filters redraws the lines, in shades per trip", async ({ page, request }) => {
  const tag = `shade${Math.random().toString(36).slice(2, 8)}`;
  const first = await nameOf(request, await ownTrip(request, `${tag} One`, "hiking"));
  await ownTrip(request, `${tag} Two`, "hiking");
  await page.goto(listOf(tag));

  // Two hikes: the activity's color and its next shade (US-72).
  await expect(lines(page)).toHaveCount(2);
  const strokes = await lines(page).evaluateAll((paths) =>
    paths.map((path) => path.getAttribute("stroke")).sort(),
  );
  expect(strokes).toEqual(["#9b2543", "#b2182b"]);

  await page.getByRole("searchbox").fill(first);
  await expect(lines(page)).toHaveCount(1);
});

test("while armed, a line opens nothing and a drag from it draws the region", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Armed Walk", "hiking");
  await page.goto(listOf(await nameOf(request, id)));
  await expect(lines(page)).toHaveCount(1);

  // The drag really starts on the line: unarmed, that point is the line.
  await page.locator("#region-map").scrollIntoViewIfNeeded();
  const hit = await onTheLine(lines(page));
  expect(
    await page.evaluate(
      ({ x, y }) => document.elementFromPoint(x, y)?.classList.contains("trip-line"),
      hit,
    ),
  ).toBe(true);

  // Arming may scroll the page to the button: read the point again after.
  await page.locator("#region-select").click();
  await page.locator("#region-map").scrollIntoViewIfNeeded();
  const at = await onTheLine(lines(page));

  // A tap on the line is a tap on the map: no region, and no trip opened.
  await page.mouse.click(at.x, at.y);
  // Nothing to wait *for* when nothing should happen: give opening the trip
  // the time it would take, then look.
  await page.waitForTimeout(300);
  await expect(page).toHaveURL(/\/app\/\?/);
  await expect(page.locator("#region-select")).toHaveAttribute("aria-pressed", "true");

  await page.mouse.move(at.x, at.y);
  await page.mouse.down();
  await page.mouse.move(at.x + 120, at.y + 80, { steps: 8 });
  await page.mouse.up();

  await expect(page).toHaveURL(/[?&]bbox=/);
  await expect(page).toHaveURL(/\/app\/\?/);
});
