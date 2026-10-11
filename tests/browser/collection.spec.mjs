// Importing a guide book as a collection, and browsing it (US-86), driven
// as the owner drives it.
//
// Scope rule, from ADR-0012's 2026-08-26b amendment: both exemptions apply.
// **Real user events** — choosing Guide book and a file in the import
// screen's picker, clicking a line, hovering a waypoint, confirming a
// delete. **JS-interop rendering** — the collection's map and a track's map
// are Leaflet's. What the screens render is asserted in
// `crates/ui-dioxus/src/collection/tests.rs`, `collection_track/tests.rs`
// and `import/guide_book.rs`, and what the archive stores in
// `tests/it/us86_collections.rs`.
import { expect, signIn, test } from "./session.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { chooseActivity } from "./trips.mjs";

const fixture = (name) =>
  readFileSync(fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url)));

/// Three tracks — one without a name — and two waypoints, under the file's
/// own name and description.
const GUIDE_BOOK_GPX = fixture("guide_book.gpx");

/// One tour, whose two waypoints belong to its track.
const GUIDE_SINGLE_GPX = fixture("guide_single.gpx");

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

/// Seed a collection through the real API and return its id.
async function importCollection(request, gpx, fields = {}) {
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: { name: "book.gpx", mimeType: "application/gpx+xml", buffer: gpx },
      ...fields,
    },
  });
  expect(response.status(), "importing a guide book").toBe(201);
  const { id } = await response.json();
  created.push(id);
  return id;
}

test("a guide book's file is imported as a collection and lands on its page (US-86)", async ({
  page,
  request,
}) => {
  await page.goto("/app/import");
  await page.getByLabel("Guide book").check();
  await page.locator("#import-gpx").setInputFiles({
    name: "Lakes.gpx",
    mimeType: "application/gpx+xml",
    buffer: GUIDE_BOOK_GPX,
  });

  // What the file says about itself, prefilled.
  await expect(page.locator("#guide-book-counts")).toHaveText(
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

  await page.waitForURL(/\/app\/collections\/\d+$/);
  const id = Number(page.url().match(/\/collections\/(\d+)/)[1]);
  created.push(id);
  await expect(page.locator("#collection-name")).toHaveText(name);
  await expect(page.locator("#collection-tracks tbody tr")).toHaveCount(3);

  // Every track a line, every waypoint a marker of its own, on one map.
  await expect(page.locator("#overview-map path.leaflet-interactive")).toHaveCount(3);
  await expect(page.locator("#overview-map .waypoint-mark")).toHaveCount(2);

  const stored = await (await request.get(`/api/collections/${id}`)).json();
  expect(stored.activity_type).toBe("cycling");
});

test("a waypoint is named on hover, and a line opens its track (US-86)", async ({
  page,
  request,
}) => {
  const id = await importCollection(request, GUIDE_BOOK_GPX);

  await page.goto(`/app/collections/${id}`);
  const waypoints = page.locator("#overview-map .waypoint-mark");
  await expect(waypoints).toHaveCount(2);
  await waypoints.first().hover();
  await expect(page.locator(".leaflet-tooltip")).toHaveText("San Bernardino Pass");

  // Dispatched on the line itself, as the share's map test does.
  await page.locator("#overview-map path.leaflet-interactive").first().dispatchEvent("click");
  await page.waitForURL(/\/app\/collection-tracks\/\d+$/);
  await expect(page.locator("#trip-name")).toHaveText("Stage 1: Splügen");
  await expect(page.locator("#track-map.leaflet-container")).toBeVisible();
  // A track of a book with several has no waypoints of its own.
  await expect(page.locator("#track-map .waypoint-mark")).toHaveCount(0);

  // And the way back.
  await page.locator("#collection-of-track a").click();
  await page.waitForURL(new RegExp(`/app/collections/${id}$`));
});

test("a single tour's waypoints show on its track's map (US-86)", async ({ page, request }) => {
  const id = await importCollection(request, GUIDE_SINGLE_GPX);
  const [track] = (await (await request.get(`/api/collections/${id}`)).json()).tracks;

  await page.goto(`/app/collection-tracks/${track.id}`);

  await expect(page.locator("#track-map .waypoint-mark")).toHaveCount(2);
  await expect(page.locator("#track-map path.leaflet-interactive")).toHaveCount(1);
});

test("a track's own activity overrides the collection's (US-86)", async ({ page, request }) => {
  const id = await importCollection(request, GUIDE_SINGLE_GPX, { activity_type: "cycling" });
  const [track] = (await (await request.get(`/api/collections/${id}`)).json()).tracks;

  await page.goto(`/app/collection-tracks/${track.id}`);
  await expect(page.locator("#trip-activity")).toContainText("(the guide book's)");
  await page.locator("#edit-trip").click();
  await chooseActivity(page, "edit-activity", "hiking");
  await page.getByRole("button", { name: "Save" }).click();

  await expect(page.locator("#trip-activity")).toHaveText("Hiking");
});

test("deleting the collection is confirmed, naming what goes with it (US-86)", async ({
  page,
  request,
}) => {
  const id = await importCollection(request, GUIDE_BOOK_GPX);

  await page.goto(`/app/collections/${id}`);
  await page.locator("#delete-collection").click();
  await expect(page.locator(".confirm")).toContainText("with its 3 tracks and 2 waypoints?");
  await page.getByRole("button", { name: "Delete it" }).click();

  await page.waitForURL(/\/app\/(\?.*)?$/);
  expect((await request.get(`/api/collections/${id}`)).status()).toBe(404);
});
