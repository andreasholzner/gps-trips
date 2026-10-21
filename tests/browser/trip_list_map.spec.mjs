// The trip-list screen's region map driven as the owner drives it (US-14,
// US-63, US-92; ADR-0012's browser layer): panning, zooming and filtering to
// it. Split out of `trip_list.spec.mjs`, whose scope rule it keeps — only
// what the host-target tests cannot reach. The lines drawn once zoomed in
// are `trip_list_lines.spec.mjs`'s.
import { expect, signIn, test } from "./session.mjs";
import { bboxOf, panRegion, tileZooms, zoomBy } from "./map.mjs";
import { seedTripList } from "./trips.mjs";

const rows = (page) => page.locator("table tbody tr");

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.beforeAll(({ request }) => seedTripList(request));

// US-92: the visible area is the region while "Filter to map" is ticked.
// Panning and zooming are real user events on a map drawn by JS.
test("with Filter to map on, panning and zooming move the region (US-92)", async ({ page }) => {
  await page.goto("/app/");
  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();
  // Fitted to the fixture's short walk — every fixture trip is the same one.
  await expect(page.locator("#region-map .trip-line").first()).toBeVisible();

  const follow = page.locator("#region-follow");
  await follow.check();
  await expect(page).toHaveURL(/[?&]bbox=/);
  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();

  // Panned off the walk, the list follows the view.
  await panRegion(page, 0.05, 0.95);
  await panRegion(page, 0.05, 0.95);
  await expect(page.getByText("No trips match your filters.")).toBeVisible();

  // Zooming out brings it back into view, and into the list.
  const [west, , east] = bboxOf(page);
  await zoomBy(page, "region-map", "Zoom out", 3, 12);
  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();
  const [wider, , further] = bboxOf(page);
  expect(further - wider).toBeGreaterThan(east - west);

  // The region outlives a toggled kind, like every other filter (US-14).
  await page.getByRole("button", { name: "Planned" }).click();
  await expect(page).toHaveURL(/kind=recorded%2Cplanned/);
  await expect(page).toHaveURL(/[?&]bbox=/);
  await page.getByRole("button", { name: "Planned" }).click();

  // And a reload starts on it, still filtering.
  await page.reload();
  await expect(follow).toBeChecked();
  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();
  await expect(page).toHaveURL(/[?&]bbox=/);

  // Off again: the region goes, and the map stays where it is.
  await expect(follow).toBeEnabled();
  await panRegion(page, 0.05, 0.95);
  await panRegion(page, 0.05, 0.95);
  await expect(page.getByText("No trips match your filters.")).toBeVisible();
  await follow.uncheck();
  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();
  await expect(page).not.toHaveURL(/[?&]bbox=/);
});

test("with Filter to map off, panning narrows nothing (US-92)", async ({ page }) => {
  await page.goto("/app/");
  await expect(page.locator("#region-map .trip-line").first()).toBeVisible();

  await panRegion(page, 0.05, 0.95);
  await panRegion(page, 0.05, 0.95);
  // Nothing to wait *for* when nothing should happen: give a re-query the
  // time it would take, then look.
  await page.waitForTimeout(300);

  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();
  await expect(page).not.toHaveURL(/[?&]bbox=/);
  await expect(page.locator("#region-follow")).not.toBeChecked();
});

// The URL follows every filter (US-52), and the router scrolls to the top on
// each change of it unless told otherwise. A real scroll position, a real
// map drag and real typing — none of which the host-target tests have.
test("changing a filter or moving the map leaves the page where it is scrolled to", async ({
  page,
}) => {
  // Short enough that the map and the filters sit below the fold.
  await page.setViewportSize({ width: 1000, height: 400 });
  await page.goto("/app/");
  await expect(page.locator("#region-map .trip-line").first()).toBeVisible();
  await page.locator("#region-follow").check();
  await expect(page).toHaveURL(/[?&]bbox=/);
  const scrollY = () => page.evaluate(() => window.scrollY);
  const bbox = () => new URL(page.url()).searchParams.get("bbox");

  await page.locator("#region-map").evaluate((map) => map.scrollIntoView({ block: "center" }));
  const atMap = await scrollY();
  expect(atMap, "the map is below the fold").toBeGreaterThan(0);
  let before = bbox();
  await panRegion(page, 0.4, 0.6);
  await expect.poll(bbox).not.toBe(before);
  expect(await scrollY(), "after panning").toBe(atMap);
  // Where the zoom button is in view, so clicking it scrolls nothing itself.
  await page
    .locator("#region-map")
    .getByRole("button", { name: "Zoom out" })
    .scrollIntoViewIfNeeded();
  const atZoom = await scrollY();
  expect(atZoom, "the zoom button is below the fold").toBeGreaterThan(0);
  before = bbox();
  await zoomBy(page, "region-map", "Zoom out", 1, await currentZoom(page));
  await expect.poll(bbox).not.toBe(before);
  expect(await scrollY(), "after zooming").toBe(atZoom);

  await page.getByText("More filters").click();
  const minKm = page.getByLabel("Min km");
  await minKm.scrollIntoViewIfNeeded();
  await page.evaluate(() => window.scrollBy(0, 40));
  const typed = await scrollY();
  expect(typed, "the field is below the fold").toBeGreaterThan(0);
  await minKm.pressSequentially("1");
  await expect(page).toHaveURL(/[?&]min_dist=1/);
  expect(await scrollY()).toBe(typed);
});

/// The region map's zoom level, from its tiles.
async function currentZoom(page) {
  const zooms = await tileZooms(page, "region-map");
  return Math.max(...zooms.map(Number));
}

// The toolbar's "Clear filters" clears the region too, so it must untick
// "Filter to map" with it (US-92).
test("clearing the filters stops filtering to the map (US-92)", async ({ page }) => {
  await page.goto("/app/?bbox=-30,30,-20,40");
  await expect(page.getByText("No trips match your filters.")).toBeVisible();
  await expect(page.locator("#region-follow")).toBeChecked();

  await page.getByRole("button", { name: "Clear filters" }).click();

  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();
  await expect(page.locator("#region-follow")).not.toBeChecked();
  await expect(page).not.toHaveURL(/[?&]bbox=/);
});

// US-58: the same control on the other map. The fix belongs to the control
// rather than to either screen, so it is asserted on both — a rule scoped to
// one screen's container would leave this one broken.
test("the region map's zoom control keeps its own size (US-58)", async ({ page }) => {
  await page.goto("/app/");
  await expect(page.locator("#region-map.leaflet-container")).toBeVisible();

  const bar = await page.locator("#region-map .leaflet-control-zoom").boundingBox();
  for (const label of ["Zoom in", "Zoom out"]) {
    const button = await page.getByRole("button", { name: label }).boundingBox();
    expect(Math.round(button.width), `${label} width`).toBe(30);
    expect(Math.round(button.height), `${label} height`).toBe(30);
    expect(button.y + button.height, `${label} inside the bar`).toBeLessThanOrEqual(
      bar.y + bar.height + 1,
    );
  }
});

// US-63: the marks are drawn by JS through `document::eval`, so only a real
// page shows them — one per matching trip, redrawn as the filters change.
test("the map marks every trip the filters match (US-63)", async ({ page }) => {
  const marks = page.locator("#region-map .heat-mark");
  await page.goto("/app/");
  // Every fixture trip is the same short walk, so the map fits to it zoomed
  // in past where it draws lines (US-73); marks are what it shows further out.
  await expect(page.locator("#region-map .trip-line").first()).toBeVisible();
  await zoomBy(page, "region-map", "Zoom out", 6, 12);

  await expect(marks).toHaveCount(2);
  // Each in its trip's activity colour, named under the map (US-75).
  await expect(page.locator('#region-map .heat-mark[fill="#b2182b"]')).toHaveCount(1);
  await expect(page.locator('#region-map .heat-mark[fill="#1f4e9c"]')).toHaveCount(1);
  await expect(page.locator(".map-legend li")).toHaveText(["Hiking", "Cycling"]);

  await page.getByRole("searchbox").fill("inn");
  await expect(rows(page)).toHaveCount(1);
  await expect(marks).toHaveCount(1);
  // A single activity needs no legend.
  await expect(page.locator(".map-legend")).toHaveCount(0);
});

// US-63: the view fits the marks once, then stays where the owner puts it
// until they ask. Panning is a real mouse gesture.
test("the map fits the trips on load and again when asked (US-63)", async ({ page }) => {
  // A mark, or zoomed in as far as the fixture's short walk fits, a line.
  const mark = page.locator("#region-map :is(.heat-mark, .trip-line)").first();
  const map = page.locator("#region-map");
  await page.goto("/app/");
  await expect(mark).toBeInViewport();

  // Pan the marks off the map.
  await panRegion(page, 0.2, 0.9);
  const inside = async () => {
    // Zoomed in, a line panned out of view is taken off the map (US-73).
    if ((await mark.count()) === 0) return false;
    const at = await mark.boundingBox();
    const now = await map.boundingBox();
    return at !== null && at.x >= now.x && at.x + at.width <= now.x + now.width;
  };
  await expect.poll(inside).toBe(false);

  await page.locator("#region-fit").click();
  await expect.poll(inside).toBe(true);
});
