// Importing a collection, browsing it (US-86) and tagging it
// and its tracks (US-93), driven as the owner drives it.
//
// Scope rule, from ADR-0012's 2026-08-26b amendment: both exemptions apply.
// **Real user events** — choosing Collection and a file in the import
// screen's picker, clicking a line, hovering a waypoint, confirming a
// delete or a new tag. **JS-interop rendering** — the collection's map and
// a track's map are Leaflet's. What the screens render is asserted in
// `crates/ui-dioxus/src/collection/tests.rs`, `collection_track/tests.rs`
// and `import/collection.rs`, which icon a symbol gets in
// `waypoint_icon.rs` (US-95), and what the archive stores in
// `tests/it/us86_collections.rs` and `us95_waypoint_symbols.rs`.
import { expect, signIn, test } from "./session.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { clickLine, hintRows } from "./map.mjs";
import { chooseActivity } from "./trips.mjs";

const fixture = (name) =>
  readFileSync(fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url)));

/// Three tracks — one without a name — and two waypoints, the first with a
/// symbol (`Triangle, Blue`), under the file's own name and description.
const COLLECTION_GPX = fixture("collection.gpx");

/// One tour, whose two waypoints belong to its track; the first, with the
/// symbol `Parking Area`.
const COLLECTION_SINGLE_GPX = fixture("collection_single.gpx");

/// A waypoint's marker, whether its symbol's icon (US-95) or the plain one.
const waypointsOn = (page, map) => page.locator(`${map} .waypoint-mark, ${map} .waypoint-symbol`);

/// Every collection these tests create, taken away again afterwards: the
/// suite shares one archive.
const created = [];

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  for (const id of created.splice(0)) {
    await request.delete(`/api/collections/${id}`);
  }
});

/// Seed a collection through the real API and return its id. No two
/// collections share a name (US-86), and every spec seeds the same files
/// into one archive, so each gets a name of its own unless one is given.
async function importCollection(request, gpx, fields = {}) {
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: { name: "book.gpx", mimeType: "application/gpx+xml", buffer: gpx },
      name: `Book ${Math.random().toString(36).slice(2, 8)}`,
      ...fields,
    },
  });
  expect(response.status(), "importing a collection").toBe(201);
  const { id } = await response.json();
  created.push(id);
  return id;
}

test("a GPX file is imported as a collection and lands on its page (US-86)", async ({
  page,
  request,
}) => {
  await page.goto("/app/import");
  await page.getByLabel("Collection", { exact: true }).check();
  await page.locator("#import-gpx").setInputFiles({
    name: "Lakes.gpx",
    mimeType: "application/gpx+xml",
    buffer: COLLECTION_GPX,
  });

  // What the file says about itself, prefilled.
  await expect(page.locator("#collection-counts")).toHaveText(
    "The file holds 3 tracks and 2 waypoints.",
  );
  await expect(page.locator("#collection-name-field")).toHaveValue("Lakes & Passes");
  await expect(page.locator("#collection-description-field")).toHaveValue(
    "Ten tours around the lakes.\n\nBruckmann, Baukasten",
  );
  const name = `Lakes ${Math.random().toString(36).slice(2, 8)}`;
  await page.locator("#collection-name-field").fill(name);
  await chooseActivity(page, "collection-activity", "cycling");
  await page.locator("#import-confirm").click();

  await page.waitForURL(/\/app\/collections\/\d+\??$/);
  const id = Number(page.url().match(/\/collections\/(\d+)/)[1]);
  created.push(id);
  await expect(page.locator("#collection-name")).toHaveText(name);
  await expect(page.locator("#collection-tracks tbody tr")).toHaveCount(3);

  // Every track a line, every waypoint a marker of its own, on one map.
  await expect(page.locator("#overview-map path.leaflet-interactive")).toHaveCount(3);
  await expect(waypointsOn(page, "#overview-map")).toHaveCount(2);

  const stored = await (await request.get(`/api/collections/${id}`)).json();
  expect(stored.activity_type).toBe("cycling");
});

test("a waypoint is named on hover, and a line opens its track (US-86)", async ({
  page,
  request,
}) => {
  const id = await importCollection(request, COLLECTION_GPX);

  await page.goto(`/app/collections/${id}`);
  const waypoints = waypointsOn(page, "#overview-map");
  await expect(waypoints).toHaveCount(2);
  // The pass ends the first stage and starts the second: the hint names all
  // three, the lines first (US-104).
  await waypoints.first().hover();
  await expect(hintRows(page, "overview-map")).toHaveText([
    "Stage 1: Splügen",
    /2$/,
    "San Bernardino Pass",
  ]);

  await clickLine(page, page.locator("#overview-map path.leaflet-interactive").first());
  await page.waitForURL(/\/app\/collection-tracks\/\d+$/);
  await expect(page.locator("#trip-name")).toHaveText("Stage 1: Splügen");
  await expect(page.locator("#track-map.leaflet-container")).toBeVisible();
  // A track of a collection with several has no waypoints of its own.
  await expect(waypointsOn(page, "#track-map")).toHaveCount(0);

  // And the way back.
  await page.locator("#collection-of-track a").click();
  await page.waitForURL(new RegExp(`/app/collections/${id}\\??$`));
});

test("a single tour's waypoints show on its track's map (US-86)", async ({ page, request }) => {
  const id = await importCollection(request, COLLECTION_SINGLE_GPX);
  const [track] = (await (await request.get(`/api/collections/${id}`)).json()).tracks;

  await page.goto(`/app/collection-tracks/${track.id}`);

  await expect(waypointsOn(page, "#track-map")).toHaveCount(2);
  await expect(page.locator("#track-map path.leaflet-interactive")).toHaveCount(1);
});

test("a waypoint with a symbol shows its icon, named on hover; one without keeps the marker (US-95)", async ({
  page,
  request,
}) => {
  const id = await importCollection(request, COLLECTION_GPX);

  await page.goto(`/app/collections/${id}`);
  // `Triangle, Blue`: the shape in Garmin's blue.
  const symbol = page.locator("#overview-map .waypoint-symbol");
  await expect(symbol).toHaveCount(1);
  await expect(symbol.locator("path.shape")).toHaveAttribute("fill", "#1f7fe0");
  await expect(page.locator("#overview-map .waypoint-mark")).toHaveCount(1);
  // Named in the map's hint after its icon (US-104).
  await symbol.hover();
  const named = hintRows(page, "overview-map").filter({ hasText: "San Bernardino Pass" });
  await expect(named.locator(".waypoint-symbol path.shape")).toHaveAttribute("fill", "#1f7fe0");
});

test("a navaid shows its diamond, a two-tone one split in its colors (US-95)", async ({
  page,
  request,
}) => {
  const gpx = `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="trip-archive-test" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="46.4958" lon="9.1705"><name>Pass</name><sym>Navaid, Green/Red</sym></wpt>
  <wpt lat="46.5822" lon="9.8377"><name>Col</name><sym>Circle with X</sym></wpt>
  <trk><trkseg>
    <trkpt lat="46.4958" lon="9.1705"></trkpt><trkpt lat="46.5822" lon="9.8377"></trkpt>
  </trkseg></trk>
</gpx>`;
  const id = await importCollection(request, Buffer.from(gpx));

  await page.goto(`/app/collections/${id}`);
  const symbols = page.locator("#overview-map .waypoint-symbol");
  await expect(symbols).toHaveCount(2);
  await expect(page.locator("#overview-map path.shape")).toHaveAttribute("fill", "#2e9a3e");
  await expect(page.locator("#overview-map path.half")).toHaveAttribute("fill", "#e0322b");
  await expect(page.locator("#overview-map path.glyph")).toHaveCount(1);
});

// A marker turned in CSS was turned about the map pane's corner, Leaflet's
// placing transform and all, and landed far to the south-west of its place.
test("a waypoint without an icon sits where it is (US-86)", async ({ page, request }) => {
  const id = await importCollection(request, COLLECTION_GPX);

  await page.goto(`/app/collections/${id}`);
  // Albula Pass ends Stage 3, which runs down and to the right to it.
  const mark = await page.locator("#overview-map .waypoint-mark").boundingBox();
  const stage3 = await page.locator("#overview-map path.leaflet-interactive").nth(2).boundingBox();

  expect(Math.abs(mark.x + mark.width / 2 - (stage3.x + stage3.width))).toBeLessThan(3);
  expect(Math.abs(mark.y + mark.height / 2 - (stage3.y + stage3.height))).toBeLessThan(3);
});

test("a collection track's map shows its waypoints' icons too (US-95)", async ({
  page,
  request,
}) => {
  const id = await importCollection(request, COLLECTION_SINGLE_GPX);
  const [track] = (await (await request.get(`/api/collections/${id}`)).json()).tracks;

  await page.goto(`/app/collection-tracks/${track.id}`);

  // `Parking Area`: a glyph on a badge.
  const symbol = page.locator("#track-map .waypoint-symbol");
  await expect(symbol).toHaveCount(1);
  await expect(symbol.locator("circle.badge")).toHaveCount(1);
  await expect(symbol.locator("path.glyph")).toHaveCount(1);
  await expect(page.locator("#track-map .waypoint-mark")).toHaveCount(1);
});

test("a track's name keeps a space from its activity's icon (US-86)", async ({ page, request }) => {
  const id = await importCollection(request, COLLECTION_GPX, { activity_type: "cycling" });

  await page.goto(`/app/collections/${id}`);
  const link = page.locator("#collection-tracks tbody a").first();
  await expect(link.locator(".activity-icon")).toHaveCount(1);
  const [iconRight, textLeft] = await link.evaluate((a) => {
    const text = document.createRange();
    text.selectNodeContents(a.lastChild);
    return [
      a.querySelector(".activity-icon").getBoundingClientRect().right,
      text.getBoundingClientRect().left,
    ];
  });

  expect(textLeft - iconRight).toBeGreaterThanOrEqual(4);
});

test("a track's own activity overrides the collection's (US-86)", async ({ page, request }) => {
  const id = await importCollection(request, COLLECTION_SINGLE_GPX, { activity_type: "cycling" });
  const [track] = (await (await request.get(`/api/collections/${id}`)).json()).tracks;

  await page.goto(`/app/collection-tracks/${track.id}`);
  await expect(page.locator("#trip-activity")).toHaveText("Cycling");
  await page.locator("#edit-trip").click();
  // Taken from the collection: its entry, with the collection's icon.
  await expect(page.locator("#edit-activity")).toHaveText("Collection");
  await expect(page.locator('#edit-activity .activity-icon[title="Cycling"]')).toHaveCount(1);
  await chooseActivity(page, "edit-activity", "hiking");
  await page.getByRole("button", { name: "Save" }).click();

  await expect(page.locator("#trip-activity")).toHaveText("Hiking");
});

// US-93: tagged as a trip is (US-33) — a new tag only after it is
// confirmed — and a collection's tags are its own, not its tracks'.
test("a collection and its track are tagged as a trip is, each with its own tags (US-93)", async ({
  page,
  request,
}) => {
  const id = await importCollection(request, COLLECTION_GPX);
  const [track] = (await (await request.get(`/api/collections/${id}`)).json()).tracks;
  const name = `book-${Math.random().toString(36).slice(2, 8)}`;

  await page.goto(`/app/collections/${id}`);
  await expect(page.locator(".trip-tags .chip")).toHaveCount(0);
  await page.getByRole("button", { name: "Add tag" }).click();
  await page.locator("#tag-input").fill(name);
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText(`Create a new tag "${name}"?`)).toBeVisible();
  expect(await (await request.get(`/api/collections/${id}/tags`)).json()).toEqual([]);
  await page.getByRole("button", { name: "Create it" }).click();

  // Summaries are about trips: the chip leads nowhere.
  const chip = page.locator(".trip-tags .chip", { hasText: name });
  await expect(chip).toBeVisible();
  await expect(chip.locator("a")).toHaveCount(0);

  // The track has none of its collection's; a known name needs no
  // confirmation.
  await page.goto(`/app/collection-tracks/${track.id}`);
  await expect(page.locator(".trip-tags .chip")).toHaveCount(0);
  await page.getByRole("button", { name: "Add tag" }).click();
  await page.locator("#tag-input").fill(name);
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.locator(".trip-tags .chip", { hasText: name })).toBeVisible();

  await page.getByTitle(`Remove ${name}`).click();
  await expect(page.locator(".trip-tags .chip")).toHaveCount(0);
  expect(await (await request.get(`/api/collection-tracks/${track.id}/tags`)).json()).toEqual([]);
  const onCollection = await (await request.get(`/api/collections/${id}/tags`)).json();
  expect(onCollection.map((tag) => tag.name)).toEqual([name]);
});

test("deleting the collection is confirmed, naming what goes with it (US-86)", async ({
  page,
  request,
}) => {
  const id = await importCollection(request, COLLECTION_GPX);

  await page.goto(`/app/collections/${id}`);
  await page.locator("#delete-collection").click();
  await expect(page.locator(".confirm")).toContainText("with its 3 tracks and 2 waypoints?");
  await page.getByRole("button", { name: "Delete it" }).click();

  await page.waitForURL(/\/app\/(\?.*)?$/);
  expect((await request.get(`/api/collections/${id}`)).status()).toBe(404);
});
