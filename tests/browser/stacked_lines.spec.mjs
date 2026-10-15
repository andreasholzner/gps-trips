// What lies under the pointer on a map whose lines and waypoints open a page
// (US-104): the hint naming all of it, and the menu to choose one from.
//
// Hovering, clicking, tapping, pressing Escape and moving the map are real
// user events on maps drawn by JS through `document::eval` — both of this
// layer's exemptions (ADR-0012). What each line and waypoint is called, its
// icon and whether it opens anything are asserted in `crates/ui-dioxus`
// (`interop`, `trip_lines`, `linked`, `shared`, `waypoint`).
import { expect, signIn, test } from "./session.mjs";
import { clickLine, hintRows, menuEntries, menuRows, onTheLine } from "./map.mjs";
import { ownTrips, SAMPLE_GPX } from "./trips.mjs";

const ownTrip = ownTrips(test);

/// A word no other spec's names hold: the suite shares one archive.
const word = () => `stack${Math.random().toString(36).slice(2, 8)}`;

const regionLines = (page) => page.locator("#region-map .trip-line");

/// One track and two waypoints: a hut away from the track, and a pass at
/// its end.
const HUT_GPX = Buffer.from(`<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="trip-archive-test" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="46.5300" lon="9.2400"><name>Lone hut</name></wpt>
  <wpt lat="46.4958" lon="9.1705"><name>Pass</name><sym>Summit</sym></wpt>
  <trk><name>Climb</name><trkseg>
    <trkpt lat="46.5500" lon="9.3200"></trkpt><trkpt lat="46.4958" lon="9.1705"></trkpt>
  </trkseg></trk>
</gpx>`);

const collections = [];
const plans = [];

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  for (const id of collections.splice(0)) await request.delete(`/api/collections/${id}`);
  for (const id of plans.splice(0)) await request.delete(`/api/trips/${id}`);
});

/// Import `HUT_GPX` as a collection; its id and its waypoints by name.
async function hutCollection(request) {
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: { name: "huts.gpx", mimeType: "application/gpx+xml", buffer: HUT_GPX },
      name: word(),
    },
  });
  expect(response.status(), "importing the collection").toBe(201);
  const { id } = await response.json();
  collections.push(id);
  const waypoints = await (await request.get(`/api/collections/${id}/waypoints`)).json();
  return { id, waypoint: (name) => waypoints.find((w) => w.name === name) };
}

/// The sample walk imported as a plan; its id.
async function importPlan(request, name) {
  const response = await request.post("/api/import", {
    maxRedirects: 0,
    multipart: {
      gpx: { name: "track.gpx", mimeType: "application/gpx+xml", buffer: SAMPLE_GPX },
      name,
      kind: "planned",
      activity_type: "hiking",
    },
  });
  expect(response.status(), `importing ${name}`).toBe(303);
  const id = Number(response.headers()["location"].replace("/app/trips/", ""));
  plans.push(id);
  return id;
}

/// A waypoint's marker on the map in `#container`, by its place in the list.
const marker = (page, container, n) =>
  page
    .locator(
      `#${container} :is(.leaflet-marker-pane .waypoint-mark, .leaflet-marker-pane .waypoint-symbol)`,
    )
    .nth(n);

test("stacked lines are named five at most, each after its icon, and a click offers every one (US-104)", async ({
  page,
  request,
}) => {
  const tag = word();
  // Every fixture track is the same walk: six lines on one another.
  for (let i = 1; i <= 5; i++) await ownTrip(request, `${tag} Walk ${i}`, "hiking");
  await ownTrip(request, `${tag} Unspecified`);
  await page.goto(`/app/?q=${tag}`);
  await expect(regionLines(page)).toHaveCount(6);

  const at = await onTheLine(regionLines(page).first());
  await page.mouse.move(at.x, at.y);
  const rows = hintRows(page, "region-map");
  await expect(rows).toHaveCount(6);
  await expect(rows.last()).toHaveText("…");

  // The menu lists all six; a hike leads with its icon, the unspecified
  // walk with its name alone.
  await page.mouse.click(at.x, at.y);
  const entries = menuEntries(page, "region-map");
  await expect(entries).toHaveCount(6);
  await expect(entries.filter({ has: page.locator(".pick-icon") })).toHaveCount(5);
  await expect(
    entries.filter({ hasText: `${tag} Walk 1` }).locator('path[fill="#b2182b"]'),
  ).not.toHaveCount(0);
  await expect(entries.filter({ hasText: `${tag} Unspecified` }).locator(".pick-icon")).toHaveCount(
    0,
  );
  await expect(rows).toHaveCount(0);

  // Escape closes it and opens nothing.
  const list = page.url();
  await page.keyboard.press("Escape");
  await expect(entries).toHaveCount(0);
  expect(page.url()).toBe(list);

  // So does a click elsewhere, which opens no menu of its own either.
  await page.mouse.click(at.x, at.y);
  await expect(entries).toHaveCount(6);
  const map = await page.locator("#region-map").boundingBox();
  await page.mouse.click(map.x + 20, map.y + map.height - 20);
  await expect(entries).toHaveCount(0);
  await page.mouse.click(at.x, at.y);
  await expect(entries).toHaveCount(6);
  await page.mouse.click(at.x, at.y);
  await expect(entries).toHaveCount(0);
  expect(page.url()).toBe(list);

  // And so does moving the map — which draws the lines anew, so last.
  await page.mouse.click(at.x, at.y);
  await expect(entries).toHaveCount(6);
  await page.mouse.move(map.x + 20, map.y + map.height - 20);
  await page.mouse.wheel(0, 200);
  await expect(entries).toHaveCount(0);
  expect(page.url()).not.toMatch(/\/trips\//);
});

test("choosing an entry opens its trip (US-104)", async ({ page, request }) => {
  const tag = word();
  await ownTrip(request, `${tag} First`, "hiking");
  const second = await ownTrip(request, `${tag} Second`, "cycling");
  await page.goto(`/app/?q=${tag}`);
  await expect(regionLines(page)).toHaveCount(2);

  await clickLine(page, regionLines(page).first());
  const entries = menuEntries(page, "region-map");
  await expect(entries).toHaveCount(2);
  await entries.filter({ hasText: `${tag} Second` }).click();
  await expect(page).toHaveURL(new RegExp(`/app/trips/${second}$`));
});

test("a recorded line is as easy to point at as a planned one (US-104)", async ({
  page,
  request,
}) => {
  const tag = word();
  const recorded = await ownTrip(request, `${tag} Recorded`, "cycling");
  await importPlan(request, `${tag} Planned`);
  await page.goto(`/app/?q=${tag}&kind=recorded%2Cplanned`);
  await expect(regionLines(page)).toHaveCount(2);

  // A few pixels beside the walk both lie on, not on either stroke: the
  // thin solid line is named as the dashed one is.
  const solid = page.locator("#region-map .trip-line:not([stroke-dasharray])");
  const at = await onTheLine(solid);
  await page.mouse.move(at.x, at.y + 5);
  const rows = hintRows(page, "region-map");
  await expect(rows).toHaveCount(2);
  await expect(rows.filter({ hasText: `${tag} Recorded` })).toHaveCount(1);
  await expect(rows.filter({ hasText: `${tag} Planned` })).toHaveCount(1);
  // The first named stands out, though the pointer is on neither stroke.
  await expect(page.locator('#region-map .trip-line[stroke-width="6"]')).toHaveCount(1);

  // And a click there offers it too.
  await page.mouse.click(at.x, at.y + 5);
  await menuEntries(page, "region-map")
    .filter({ hasText: `${tag} Recorded` })
    .click();
  await expect(page).toHaveURL(new RegExp(`/app/trips/${recorded}$`));
});

test("a waypoint opens its page alone and from the menu, but not on its own page (US-104)", async ({
  page,
  request,
}) => {
  const { id, waypoint } = await hutCollection(request);
  const hut = waypoint("Lone hut");
  const pass = waypoint("Pass");

  // Alone, a click opens it at once.
  await page.goto(`/app/collections/${id}`);
  await expect(marker(page, "overview-map", 0)).toBeVisible();
  await marker(page, "overview-map", 0).click();
  await expect(page).toHaveURL(new RegExp(`/app/waypoints/${hut.id}$`));

  // On its own page it is named, but opens nothing.
  await expect(marker(page, "overview-map", 0)).toBeVisible();
  await marker(page, "overview-map", 0).hover();
  await expect(hintRows(page, "overview-map")).toHaveText(["Lone hut"]);
  await marker(page, "overview-map", 0).click();
  await expect(menuRows(page, "overview-map")).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`/app/waypoints/${hut.id}$`));

  // At the track's end, the pass lies on its line: both are offered.
  await page.goto(`/app/collections/${id}`);
  const passMarker = page.locator("#overview-map .leaflet-marker-pane .waypoint-symbol");
  await expect(passMarker).toBeVisible();
  await passMarker.click();
  await expect(menuEntries(page, "overview-map")).toHaveText(["Climb", "Pass"]);
  await menuEntries(page, "overview-map").filter({ hasText: "Pass" }).click();
  await expect(page).toHaveURL(new RegExp(`/app/waypoints/${pass.id}$`));
});

test("a recipient's waypoint is named but opens nothing (US-104)", async ({
  page,
  request,
  browser,
  baseURL,
}) => {
  const { id } = await hutCollection(request);
  const created = await request.post("/api/shares", { data: { collection_ids: [id] } });
  expect(created.status(), "creating the share").toBe(201);
  const { token } = await created.json();

  const context = await browser.newContext({ baseURL });
  const recipient = await context.newPage();
  await recipient.goto(`/app/s/${token}/collections/${id}`);
  const hut = marker(recipient, "overview-map", 0);
  await expect(hut).toBeVisible();
  await hut.hover();
  await expect(hintRows(recipient, "overview-map")).toHaveText(["Lone hut"]);
  const shared = recipient.url();
  await hut.click();
  await expect(menuRows(recipient, "overview-map")).toHaveCount(0);
  expect(recipient.url()).toBe(shared);
  await context.close();
});

test("a ticked link's line is named with the page's own track, and opens its page (US-104)", async ({
  page,
  request,
}) => {
  const tag = word();
  const day = await ownTrip(request, `${tag} Day`, "hiking");
  const plan = await importPlan(request, `${tag} Plan`);
  const linked = await request.post("/api/links", {
    data: { trip_ids: [day, plan], collection_track_ids: [] },
  });
  expect(linked.status(), "linking").toBe(200);
  const planName = (await (await request.get(`/api/trips/${plan}`)).json()).name;
  const dayName = (await (await request.get(`/api/trips/${day}`)).json()).name;

  await page.goto(`/app/trips/${day}`);
  const own = page.locator("#track-map path.leaflet-interactive");
  await expect(own).toHaveCount(1);
  await page.getByRole("checkbox", { name: `Show ${planName} on the map` }).check();
  await expect(own).toHaveCount(2);

  // The same walk: the hint names the day itself, then the plan.
  const at = await onTheLine(own.first());
  await page.mouse.move(at.x, at.y);
  await expect(hintRows(page, "track-map")).toHaveText([dayName, planName]);

  // The day opens nothing, so a click opens the plan at once.
  await page.mouse.click(at.x, at.y);
  await expect(page).toHaveURL(new RegExp(`/app/trips/${plan}$`));
});

/// Share six trips on the same walk, each with a long name; the recipient's
/// page on the share's map.
async function longNamedShare(request, browser, baseURL, device = {}) {
  const tag = word();
  const ids = [];
  for (let i = 1; i <= 6; i++) {
    ids.push(
      await ownTrip(
        request,
        `${tag} ${i} Blick auf die Tromsø-Brücke – Eismeerkathedrale Runde von Myreng und zurück`,
        "cycling",
      ),
    );
  }
  const created = await request.post("/api/shares", { data: { trip_ids: ids } });
  expect(created.status(), "creating the share").toBe(201);
  const { token } = await created.json();
  const context = await browser.newContext({ baseURL, ...device });
  const page = await context.newPage();
  await page.goto(`/app/s/${token}`);
  await expect(page.locator("#overview-map path.leaflet-interactive")).toHaveCount(6);
  return { page, context };
}

/// Drag the map so that the walk's middle lies near its top right corner;
/// where it now is.
async function dragToCorner(page, line) {
  const at = await onTheLine(line);
  const map = await page.locator("#overview-map").boundingBox();
  const to = { x: map.x + map.width - 24, y: map.y + 24 };
  await page.mouse.move(at.x, at.y);
  await page.mouse.down();
  await page.mouse.move((at.x + to.x) / 2, (at.y + to.y) / 2, { steps: 5 });
  await page.mouse.move(to.x, to.y, { steps: 5 });
  // Held still before letting go, so Leaflet's inertia leaves the map
  // where it was dragged to.
  await page.waitForTimeout(200);
  await page.mouse.up();
  return to;
}

/// Whether `box` lies wholly within the map, and none of its rows is cut.
async function expectWithinMap(page, box) {
  await expect(box).toBeVisible();
  const map = await page.locator("#overview-map").boundingBox();
  const shown = await box.boundingBox();
  expect(shown.x).toBeGreaterThanOrEqual(map.x);
  expect(shown.y).toBeGreaterThanOrEqual(map.y);
  expect(shown.x + shown.width).toBeLessThanOrEqual(map.x + map.width + 0.5);
  expect(shown.y + shown.height).toBeLessThanOrEqual(map.y + map.height + 0.5);
  const cut = await box
    .locator(".pick-row")
    .evaluateAll((rows) => rows.filter((row) => row.scrollWidth > row.clientWidth + 1).length);
  expect(cut, "rows whose names overflow").toBe(0);
}

test("in a corner, the hint and the menu stay within the map, long names and all (US-104)", async ({
  request,
  browser,
  baseURL,
}) => {
  const { page, context } = await longNamedShare(request, browser, baseURL);
  const at = await dragToCorner(
    page,
    page.locator("#overview-map path.leaflet-interactive").first(),
  );

  await page.mouse.move(at.x - 1, at.y + 1);
  await page.mouse.move(at.x, at.y);
  await expect(hintRows(page, "overview-map")).toHaveCount(6);
  await expectWithinMap(page, page.locator("#overview-map .pick-hint"));

  await page.mouse.click(at.x, at.y);
  await expect(menuEntries(page, "overview-map")).toHaveCount(6);
  await expectWithinMap(page, page.locator("#overview-map .pick-menu"));
  // Every entry can be reached, scrolling if need be.
  await menuEntries(page, "overview-map").last().scrollIntoViewIfNeeded();
  await expect(menuEntries(page, "overview-map").last()).toBeInViewport();
  await context.close();
});

const PHONE = { hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } };

test.describe("with a touchscreen", () => {
  test.use(PHONE);

  test("a tap opens the menu even for one line, and names the page's own track there (US-104)", async ({
    page,
    request,
  }) => {
    const tag = word();
    const id = await ownTrip(request, `${tag} Tapped`, "hiking");
    await page.goto(`/app/?q=${tag}`);
    await expect(regionLines(page)).toHaveCount(1);

    let at = await onTheLine(regionLines(page));
    await page.touchscreen.tap(at.x, at.y);
    const entries = menuEntries(page, "region-map");
    await expect(entries).toHaveCount(1);
    await expect(entries).toContainText(`${tag} Tapped`);
    await entries.tap();
    await expect(page).toHaveURL(new RegExp(`/app/trips/${id}$`));

    // On the trip's own page, its track is named but not offered.
    const own = page.locator("#track-map path.leaflet-interactive");
    await expect(own).toHaveCount(1);
    at = await onTheLine(own);
    await page.touchscreen.tap(at.x, at.y);
    await expect(menuRows(page, "track-map")).toHaveCount(1);
    await expect(menuRows(page, "track-map")).toContainText(`${tag} Tapped`);
    await expect(menuEntries(page, "track-map")).toHaveCount(0);
  });

  test("on a phone, the menu stays within the map, long names and all (US-104)", async ({
    request,
    browser,
    baseURL,
  }) => {
    const { page, context } = await longNamedShare(request, browser, baseURL, PHONE);
    const at = await onTheLine(page.locator("#overview-map path.leaflet-interactive").first());
    await page.touchscreen.tap(at.x, at.y);
    await expect(menuEntries(page, "overview-map")).toHaveCount(6);
    await expectWithinMap(page, page.locator("#overview-map .pick-menu"));
    await context.close();
  });
});
