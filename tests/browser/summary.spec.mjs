// US-78: the tag summary, driven as the owner drives it (ADR-0012's browser
// layer).
//
// Scope rule, from the 2026-08-26b amendment: only real user events —
// choosing and removing a tag, clicking a line or a tag on a trip's page —
// and what `document::eval` draws, the map. The figures and what the screen
// shows for given tags are asserted in `crates/ui-dioxus` (`summary`),
// without a browser.
import { expect, signIn, test } from "./session.mjs";
import { ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// A tag name of this run's own: the suite shares one archive.
const fresh = (label) => `${label}-${Math.random().toString(36).slice(2, 8)}`;

async function tag(request, id, name) {
  const response = await request.post(`/api/trips/${id}/tags`, { data: { name } });
  expect(response.status(), `tagging ${id} ${name}`).toBe(201);
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

  // Either line is one of the two trips: the fixtures are the same track.
  await lines(page).first().dispatchEvent("click");
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
