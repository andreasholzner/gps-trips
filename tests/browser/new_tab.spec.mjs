// A Ctrl- or ⌘-click on what a map opens opens it in a new tab (US-114), on
// every kind of map US-104 lists. Holding a key while clicking on a map
// drawn by JS is a real user event — this layer's exemption (ADR-0012).
// Which address each line and waypoint is given is asserted in
// `crates/ui-dioxus`; the trip list's lines in `trip_list_lines.spec.mjs`.
import { expect, signIn, test } from "./session.mjs";
import { menuEntries, onTheLine } from "./map.mjs";
import { ownTrips, SAMPLE_GPX } from "./trips.mjs";

const ownTrip = ownTrips(test);

/// A word no other spec's names hold: the suite shares one archive.
const word = () => `tab${Math.random().toString(36).slice(2, 8)}`;

/// One track and a hut away from it.
const HUT_GPX = Buffer.from(`<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="trip-archive-test" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="46.5300" lon="9.2400"><name>Lone hut</name></wpt>
  <trk><name>Climb</name><trkseg>
    <trkpt lat="46.5500" lon="9.3200"></trkpt><trkpt lat="46.4958" lon="9.1705"></trkpt>
  </trkseg></trk>
</gpx>`);

const collections = [];

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  for (const id of collections.splice(0)) await request.delete(`/api/collections/${id}`);
});

/// Import `HUT_GPX` as a collection named `name`: its id, its track's and
/// the hut's.
async function hutCollection(request, name = word()) {
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: { name: "huts.gpx", mimeType: "application/gpx+xml", buffer: HUT_GPX },
      name,
    },
  });
  expect(response.status(), "importing the collection").toBe(201);
  const { id } = await response.json();
  collections.push(id);
  const [track] = (await (await request.get(`/api/collections/${id}`)).json()).tracks;
  const [hut] = await (await request.get(`/api/collections/${id}/waypoints`)).json();
  return { id, track: track.id, hut: hut.id };
}

/// Share what `data` names; a recipient's page in a context of its own.
async function asRecipient(request, browser, baseURL, data) {
  const created = await request.post("/api/shares", { data });
  expect(created.status(), "creating the share").toBe(201);
  const { token } = await created.json();
  const context = await browser.newContext({ baseURL });
  return { token, context, page: await context.newPage() };
}

const hutOn = (page, container) =>
  page.locator(`#${container} .leaflet-marker-pane .waypoint-mark`).first();

/// Click with Ctrl held, as a desktop opens a link in a new tab; that the
/// tab opened at `url`, and this one stayed where it was.
async function opensInNewTab(page, click, url) {
  const before = page.url();
  await page.keyboard.down("Control");
  let opened;
  try {
    [opened] = await Promise.all([page.context().waitForEvent("page"), click()]);
  } finally {
    await page.keyboard.up("Control");
  }
  await expect(opened).toHaveURL(url);
  expect(page.url()).toBe(before);
  await opened.close();
}

/// A click on the middle of `line`, with the mouse.
const onLine = (page, line) => async () => {
  const at = await onTheLine(line);
  await page.mouse.click(at.x, at.y);
};

test("a collection's tracks and waypoints open in a new tab, on its map and the trip list's", async ({
  page,
  request,
}) => {
  const name = word();
  const { id, track, hut } = await hutCollection(request, name);

  await page.goto(`/app/collections/${id}`);
  await expect(hutOn(page, "overview-map")).toBeVisible();
  await opensInNewTab(
    page,
    () => hutOn(page, "overview-map").click(),
    new RegExp(`/app/waypoints/${hut}$`),
  );
  await opensInNewTab(
    page,
    onLine(page, page.locator("#overview-map path.leaflet-interactive")),
    new RegExp(`/app/collection-tracks/${track}$`),
  );

  await page.goto(`/app/?kind=collections&q=${name}`);
  await expect(hutOn(page, "region-map")).toBeVisible();
  await opensInNewTab(
    page,
    () => hutOn(page, "region-map").click(),
    new RegExp(`/app/waypoints/${hut}$`),
  );
});

test("a waypoint's track and a track's waypoint open in a new tab", async ({ page, request }) => {
  const { track, hut } = await hutCollection(request);

  await page.goto(`/app/waypoints/${hut}`);
  const line = page.locator("#overview-map path.leaflet-interactive");
  await expect(line).toHaveCount(1);
  await opensInNewTab(page, onLine(page, line), new RegExp(`/app/collection-tracks/${track}$`));

  await page.goto(`/app/collection-tracks/${track}`);
  await expect(hutOn(page, "track-map")).toBeVisible();
  await opensInNewTab(
    page,
    () => hutOn(page, "track-map").click(),
    new RegExp(`/app/waypoints/${hut}$`),
  );
});

test("a ticked link's line opens its page in a new tab", async ({ page, request }) => {
  const tag = word();
  const day = await ownTrip(request, `${tag} Day`, "hiking");
  const plan = await ownTrip(request, `${tag} Plan`, "hiking", SAMPLE_GPX, "planned");
  const linked = await request.post("/api/links", {
    data: { trip_ids: [day, plan], collection_track_ids: [] },
  });
  expect(linked.status(), "linking").toBe(200);
  const planName = (await (await request.get(`/api/trips/${plan}`)).json()).name;

  await page.goto(`/app/trips/${day}`);
  const lines = page.locator("#track-map path.leaflet-interactive");
  await expect(lines).toHaveCount(1);
  await page.getByRole("checkbox", { name: `Show ${planName} on the map` }).check();
  await expect(lines).toHaveCount(2);

  // The day's own line opens nothing: the plan's is the one to open.
  await opensInNewTab(page, onLine(page, lines.first()), new RegExp(`/app/trips/${plan}$`));
});

test("a line on a tag's summary map opens its trip in a new tab", async ({ page, request }) => {
  const tag = word();
  const walk = await ownTrip(request, "Summary Walk", "hiking");
  const tagged = await request.post(`/api/trips/${walk}/tags`, { data: { name: tag } });
  expect(tagged.status(), "tagging").toBe(201);

  await page.goto(`/app/summary?tags=${tag}`);
  const line = page.locator("#overview-map path.leaflet-interactive");
  await expect(line).toHaveCount(1);
  await opensInNewTab(page, onLine(page, line), new RegExp(`/app/trips/${walk}$`));
});

test("a recipient's lines open their shared pages in a new tab", async ({
  request,
  browser,
  baseURL,
}) => {
  // Two, so the share opens on its overview map rather than on the trip;
  // the same walk, so the two lie on each other and offer a menu.
  const first = await ownTrip(request, "Shared Walk", "hiking");
  const second = await ownTrip(request, "Shared Ride", "cycling");
  const secondName = (await (await request.get(`/api/trips/${second}`)).json()).name;
  const trips = await asRecipient(request, browser, baseURL, { trip_ids: [first, second] });
  await trips.page.goto(`/app/s/${trips.token}`);
  const tripLines = trips.page.locator("#overview-map path.leaflet-interactive");
  await expect(tripLines).toHaveCount(2);
  await onLine(trips.page, tripLines.first())();
  const entry = menuEntries(trips.page, "overview-map").filter({ hasText: secondName });
  await opensInNewTab(
    trips.page,
    () => entry.click(),
    new RegExp(`/app/s/${trips.token}/trips/${second}$`),
  );
  await trips.context.close();

  const { id, track } = await hutCollection(request);
  const shared = await asRecipient(request, browser, baseURL, { collection_ids: [id] });
  await shared.page.goto(`/app/s/${shared.token}/collections/${id}`);
  const trackLine = shared.page.locator("#overview-map path.leaflet-interactive");
  await expect(trackLine).toHaveCount(1);
  await opensInNewTab(
    shared.page,
    onLine(shared.page, trackLine),
    new RegExp(`/app/s/${shared.token}/collection-tracks/${track}$`),
  );
  await shared.context.close();
});
