// US-78: the tag summary, driven as the owner drives it (ADR-0012's browser
// layer).
//
// Scope rule, from the 2026-08-26b amendment: only real user events —
// choosing and removing a tag, clicking a line or a tag on a trip's page —
// and what `document::eval` draws, the map. The figures and what the screen
// shows for given tags are asserted in `crates/ui-dioxus` (`summary`),
// without a browser.
import { expect, signIn, test } from "./session.mjs";
import { clickLine, menuEntries } from "./map.mjs";
import { ownTrips } from "./trips.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const ownTrip = ownTrips(test);

/// One short tour near Innsbruck, whose two waypoints belong to its track.
const COLLECTION_SINGLE_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/collection_single.gpx", import.meta.url)),
);

/// Every collection these tests create, taken away again afterwards.
const collections = [];

test.afterAll(async ({ request }) => {
  for (const id of collections.splice(0)) {
    await request.delete(`/api/collections/${id}`);
  }
});

/// Seed a collection of this run's own and return its id and its track's.
async function ownCollection(request) {
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: { name: "book.gpx", mimeType: "application/gpx+xml", buffer: COLLECTION_SINGLE_GPX },
      name: fresh("Karwendel"),
    },
  });
  expect(response.status(), "importing a collection").toBe(201);
  const { id } = await response.json();
  collections.push(id);
  const { tracks } = await (await request.get(`/api/collections/${id}`)).json();
  return { id, track: tracks[0].id };
}

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// A tag name of this run's own: the suite shares one archive.
const fresh = (label) => `${label}-${Math.random().toString(36).slice(2, 8)}`;

async function tag(request, id, name, on = "trips") {
  const response = await request.post(`/api/${on}/${id}/tags`, { data: { name } });
  expect(response.status(), `tagging ${on} ${id} ${name}`).toBe(201);
}

/// Choose `name` in the search box, as typing it and pressing Enter does.
async function choose(page, name) {
  await page.locator("#summary-tag-input").fill(name);
  await page.locator("#summary-tag-input").press("Enter");
}

const lines = (page) => page.locator("#overview-map path.leaflet-interactive");

test("the menu opens the summary, and a chosen tag stays in the URL and draws its trips (US-78)", async ({
  page,
  request,
}) => {
  const alps = fresh("alps");
  const walk = await ownTrip(request, "Summed Walk", "hiking");
  const ride = await ownTrip(request, "Summed Ride", "cycling");
  await tag(request, walk, alps);
  await tag(request, ride, alps);
  await page.goto("/app/");
  const menu = page.locator("#app-menu a");
  await expect(menu.nth(1)).toHaveText("Summary");
  await expect(menu.nth(2)).toHaveText("Statistics");
  await menu.nth(1).click();

  await expect(page).toHaveURL(/\/app\/summary/);
  await choose(page, alps);

  await expect(page).toHaveURL(new RegExp(`tags=${alps}`));
  await expect(page.locator("#summary-figures")).toBeVisible();
  await expect(page.locator("#summary-trips tbody tr")).toHaveCount(2);
  // One tag: each trip in its activity's color (US-72, US-75).
  await expect(lines(page)).toHaveCount(2);
  await expect(page.locator('#overview-map path[stroke="#b2182b"]')).toHaveCount(1);
  await expect(page.locator('#overview-map path[stroke="#1f4e9c"]')).toHaveCount(1);

  await page.reload();
  await expect(page.locator("#summary-chosen")).toContainText(alps);
  await expect(lines(page)).toHaveCount(2);

  // The fixtures are the same track, so a click on it offers both (US-104),
  // and either is one of the two trips.
  await clickLine(page, lines(page).first());
  await expect(menuEntries(page, "overview-map")).toHaveCount(2);
  await menuEntries(page, "overview-map").first().click();
  await expect(page).toHaveURL(new RegExp(`/app/trips/(${walk}|${ride})$`));
});

test("several tags stand side by side in their own colors, and one can be removed (US-78)", async ({
  page,
  request,
}) => {
  const [alps, norway] = [fresh("alps"), fresh("norway")];
  const walk = await ownTrip(request, "Alps Walk", "hiking");
  const both = await ownTrip(request, "Both Ride", "cycling");
  const paddle = await ownTrip(request, "Norway Paddle", "kayaking");
  await tag(request, walk, alps);
  await tag(request, both, alps);
  await tag(request, both, norway);
  await tag(request, paddle, norway);
  await page.goto(`/app/summary?tags=${alps},${norway}`);

  await expect(page.locator("#summary-figures thead th")).toHaveText([alps, norway]);
  // A trip under both tags is drawn once, in the first tag's color.
  await expect(lines(page)).toHaveCount(3);
  await expect(page.locator('#overview-map path[stroke="#eb6834"]')).toHaveCount(2);
  await expect(page.locator('#overview-map path[stroke="#1baf7a"]')).toHaveCount(1);

  await page.locator("#summary-chosen").getByTitle(`Remove ${alps}`).click();
  await expect(page).toHaveURL(new RegExp(`tags=${norway}$`));
  await expect(page.locator("#summary-figures thead th")).toHaveText([norway]);
  await expect(lines(page)).toHaveCount(2);
});

test("a tag on a trip's page opens its summary (US-78)", async ({ page, request }) => {
  const alps = fresh("alps");
  const walk = await ownTrip(request, "Tagged Walk", "hiking");
  await tag(request, walk, alps);
  await page.goto(`/app/trips/${walk}`);

  await page.locator(".trip-tags .chip a", { hasText: alps }).click();

  await expect(page).toHaveURL(new RegExp(`/app/summary\\?tags=${alps}$`));
  await expect(page.locator("#summary-figures")).toBeVisible();
});

test("a tag's plans and collection tracks are drawn in their kinds' styles, and a dotted line opens its track (US-107)", async ({
  page,
  request,
}) => {
  const alps = fresh("alps");
  const walk = await ownTrip(request, "Done Walk", "hiking");
  const plan = await ownTrip(request, "Planned Walk", "hiking", undefined, "planned");
  const { id: collection, track } = await ownCollection(request);
  await tag(request, walk, alps);
  await tag(request, plan, alps);
  await tag(request, collection, alps, "collections");
  await page.goto(`/app/summary?tags=${alps}`);

  // The figures count the recorded walk alone; the list holds all three.
  await expect(page.locator("#summary-figures")).toBeVisible();
  await expect(page.locator("#summary-trips tbody tr")).toHaveCount(4);
  await expect(lines(page)).toHaveCount(3);
  await expect(
    page.locator("#overview-map path.leaflet-interactive:not([stroke-dasharray])"),
  ).toHaveCount(1);
  await expect(page.locator('#overview-map path[stroke-dasharray="8 6"]')).toHaveCount(1);
  const dotted = page.locator('#overview-map path[stroke-dasharray="1 7"]');
  await expect(dotted).toHaveCount(1);
  // And its waypoints with it.
  await expect(page.locator("#overview-map .leaflet-marker-icon")).toHaveCount(2);

  // On a map of the tour alone, so its line is long enough to aim at: a
  // click on it opens its track, straight away or from the menu it shares
  // with the track's waypoints (US-104).
  const tour = fresh("tour");
  await tag(request, collection, tour, "collections");
  await page.goto(`/app/summary?tags=${tour}`);
  await expect(dotted).toHaveCount(1);
  await clickLine(page, dotted);
  const entry = menuEntries(page, "overview-map").filter({ hasText: "Nordkette" });
  await Promise.race([
    page.waitForURL(/collection-tracks/),
    entry.click({ timeout: 5000 }).catch(() => {}),
  ]);
  await expect(page).toHaveURL(new RegExp(`/app/collection-tracks/${track}$`));
});

test("a tag on a collection's page and on a collection track's page opens its summary (US-107)", async ({
  page,
  request,
}) => {
  const [lakes, pass] = [fresh("lakes"), fresh("pass")];
  const { id, track } = await ownCollection(request);
  await tag(request, id, lakes, "collections");
  await tag(request, track, pass, "collection-tracks");

  await page.goto(`/app/collections/${id}`);
  await page.locator(".trip-tags .chip a", { hasText: lakes }).click();
  await expect(page).toHaveURL(new RegExp(`/app/summary\\?tags=${lakes}$`));
  await expect(page.locator("#summary-trips")).toContainText("not counted");

  await page.goto(`/app/collection-tracks/${track}`);
  await page.locator(".trip-tags .chip a", { hasText: pass }).click();
  await expect(page).toHaveURL(new RegExp(`/app/summary\\?tags=${pass}$`));
  await expect(page.locator("#summary-trips")).toContainText("not counted");
});

// Layout, which only a browser computes: the search box shares the heading's
// row, on its right.
test("the tag search sits right of the heading, level with it (US-78)", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/app/summary");

  const heading = await page.getByRole("heading", { name: "Summary", level: 1 }).boundingBox();
  const search = await page.locator("#summary-tag-input").boundingBox();
  const row = await page.locator(".summary-heading").boundingBox();
  const middle = (box) => box.y + box.height / 2;
  expect(Math.abs(middle(search) - middle(heading))).toBeLessThanOrEqual(4);
  expect(Math.abs(search.x + search.width - (row.x + row.width))).toBeLessThanOrEqual(1);
  expect(search.x).toBeGreaterThan(heading.x + heading.width);
});

// Layout, which only a browser computes: with several tags, a group's
// heading stands clear of the trips listed above it.
test("each tag's trips stand apart from the group above (US-78)", async ({ page, request }) => {
  const [alps, norway] = [fresh("alps"), fresh("norway")];
  const walk = await ownTrip(request, "Grouped Walk", "hiking");
  const paddle = await ownTrip(request, "Grouped Paddle", "kayaking");
  await tag(request, walk, alps);
  await tag(request, paddle, norway);
  await page.goto(`/app/summary?tags=${alps},${norway}`);

  const above = await page.locator("#summary-trips table").first().boundingBox();
  const heading = await page.locator("#summary-trips h3").nth(1).boundingBox();
  expect(heading.y - (above.y + above.height)).toBeGreaterThanOrEqual(24);
  // A heading sits close to its own trips, though not against them.
  const below = await page.locator("#summary-trips table").nth(1).boundingBox();
  const gap = below.y - (heading.y + heading.height);
  expect(gap).toBeGreaterThanOrEqual(4);
  expect(gap).toBeLessThanOrEqual(10);
});

// An activity's heading in the figures is its icon and its name on one line
// (US-84), on a desktop as on a phone.
for (const width of [1280, 390]) {
  test(`an activity's heading keeps its icon beside its name at ${width}px (US-84)`, async ({
    page,
    request,
  }) => {
    const alps = fresh("alps");
    await tag(request, await ownTrip(request, "Headed Walk", "hiking"), alps);
    await page.setViewportSize({ width, height: 900 });
    await page.goto(`/app/summary?tags=${alps}`);

    const heading = page.locator("th.summary-activity", { hasText: "Hiking" });
    const [icon, text] = await heading.evaluate((th) => {
      const range = document.createRange();
      range.selectNodeContents(th);
      const text = [...range.getClientRects()].at(-1);
      const icon = th.querySelector(".activity-icon").getBoundingClientRect();
      return [
        { top: icon.top, bottom: icon.bottom, right: icon.right },
        { top: text.top, bottom: text.bottom, left: text.left },
      ];
    });
    // Side by side: the name starts right of the icon, at the same height.
    expect(text.left).toBeGreaterThanOrEqual(icon.right - 1);
    expect(text.top).toBeLessThan(icon.bottom);
    expect(text.bottom).toBeGreaterThan(icon.top);
  });
}
