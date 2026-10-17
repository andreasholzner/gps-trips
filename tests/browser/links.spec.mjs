// Linking what the owner did to what they planned (US-99), driven as the
// owner drives it.
//
// Scope rule, from ADR-0012's 2026-08-26b amendment: **real user events** —
// ticking rows, the bar's Link action, removing a link, choosing a filter.
// What the pages render is asserted in `crates/ui-dioxus` (`linked.rs`,
// `link_panel.rs`, `picked.rs`, `selection_bar.rs`, `list_table.rs`,
// `filter_bar.rs`, `collection/tests/recorded.rs`), and what the archive
// answers in `tests/it/us99_links.rs` and `tests/it/us99_with_a_recorded_trip.rs`.
import { expect, signIn, test } from "./session.mjs";
import { SAMPLE_GPX } from "./trips.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const COLLECTION_SINGLE_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/collection_single.gpx", import.meta.url)),
);

/// A word no other spec's names hold, so a search lists this file's alone:
/// the suite shares one archive.
const WORD = `linked${Math.random().toString(36).slice(2, 8)}`;

const trips = [];
const collections = [];

const rows = (page) => page.locator("table tbody tr");
const MARK = "Has a recorded trip";

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  for (const id of trips.splice(0)) await request.delete(`/api/trips/${id}`);
  for (const id of collections.splice(0)) await request.delete(`/api/collections/${id}`);
});

/// Import the sample walk as a trip of `kind`, named `name`; its id.
async function importTrip(request, name, kind) {
  const response = await request.post("/api/import", {
    maxRedirects: 0,
    multipart: {
      gpx: { name: "track.gpx", mimeType: "application/gpx+xml", buffer: SAMPLE_GPX },
      name,
      kind,
    },
  });
  expect(response.status(), `importing ${name}`).toBe(303);
  const id = Number(response.headers()["location"].replace("/app/trips/", ""));
  trips.push(id);
  return id;
}

async function importCollection(request, name) {
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: { name: "book.gpx", mimeType: "application/gpx+xml", buffer: COLLECTION_SINGLE_GPX },
      name,
    },
  });
  expect(response.status(), "importing a collection").toBe(201);
  const { id } = await response.json();
  collections.push(id);
  return id;
}

test("a selected day, its plan and a stage are linked from the bar, marked, and unlinked on a page (US-99)", async ({
  page,
  request,
}) => {
  const day = await importTrip(request, `${WORD} Day`, "recorded");
  await importTrip(request, `${WORD} Plan`, "planned");
  await importCollection(request, `${WORD} Book`);
  await page.goto(`/app/?kind=recorded%2Cplanned%2Ccollections&q=${WORD}`);
  await expect(rows(page)).toHaveCount(3);
  const link = page.getByRole("button", { name: "Link…" });

  await rows(page)
    .filter({ hasText: `${WORD} Day` })
    .getByRole("checkbox")
    .check();
  await expect(link).toBeDisabled();
  await rows(page)
    .filter({ hasText: `${WORD} Plan` })
    .getByRole("checkbox")
    .check();
  const book = rows(page).filter({ hasText: `${WORD} Book` });
  await book.getByRole("button", { name: "Show its tracks" }).click();
  await rows(page).nth(3).getByRole("checkbox").check();
  await link.click();
  await expect(page.getByText("Makes 3 links.")).toBeVisible();
  await page.getByRole("button", { name: "Link", exact: true }).click();

  // The selection is cleared and the list read anew, marks and all.
  await expect(page.locator(".selection-bar")).toHaveCount(0);
  await expect(rows(page).filter({ hasText: `${WORD} Plan` })).toContainText(MARK);
  await expect(rows(page).filter({ hasText: `${WORD} Day` })).not.toContainText(MARK);

  // Narrowed to what has no recorded trip, the plan goes and the day stays.
  await page.locator("#filter-recorded-trip").selectOption("without");
  await expect(page).toHaveURL(/recorded_trip=without/);
  await expect(rows(page).filter({ hasText: `${WORD} Plan` })).toHaveCount(0);
  await expect(rows(page).filter({ hasText: `${WORD} Day` })).toHaveCount(1);

  // On the day's page, below the photos: the plan and the stage, each
  // shown on the map once ticked, in its kind's line style.
  await page.goto(`/app/trips/${day}`);
  const lines = page.locator("#track-map path.leaflet-interactive");
  await expect(lines).toHaveCount(1);
  await page.getByRole("checkbox", { name: `Show ${WORD} Plan on the map` }).check();
  await expect(lines).toHaveCount(2);
  await expect(page.locator('#track-map path[stroke-dasharray="8 6"]')).toHaveCount(1);
  await page.getByRole("checkbox", { name: `Show ${WORD} Plan on the map` }).uncheck();
  await expect(lines).toHaveCount(1);

  // One removed at once.
  const links = page.locator("#links");
  await expect(links.getByRole("link", { name: `${WORD} Plan` })).toBeVisible();
  await expect(links.locator("li")).toHaveCount(2);
  await links.getByRole("button", { name: `Remove the link to ${WORD} Plan` }).click();
  await expect(links.locator("li")).toHaveCount(1);
  await expect(links.getByRole("link", { name: `${WORD} Plan` })).toHaveCount(0);
});

test("a collection's tracks are narrowed to those with a recorded trip, in its URL (US-99)", async ({
  page,
  request,
}) => {
  const day = await importTrip(request, `${WORD} Ride`, "recorded");
  const id = await importCollection(request, `${WORD} Shelf`);
  const { tracks } = await (await request.get(`/api/collections/${id}`)).json();
  const linked = await request.post("/api/links", {
    data: { trip_ids: [day], collection_track_ids: [tracks[0].id] },
  });
  expect(linked.status()).toBe(200);

  await page.goto(`/app/collections/${id}`);
  await expect(page.locator("#collection-recorded")).toHaveText(
    "1 of 1 track with a recorded trip",
  );
  await page.locator("#collection-recorded-trip").selectOption("without");

  await expect(page).toHaveURL(/recorded_trip=without/);
  await expect(page.getByText("No track matches.")).toBeVisible();
  await page.reload();
  await expect(page.getByText("No track matches.")).toBeVisible();
});

test("ticking a linked item shows it without moving or zooming the map (US-105)", async ({
  page,
  request,
}) => {
  // The day is in Oslo and the stage near Innsbruck: framing both would
  // zoom far out.
  const day = await importTrip(request, `${WORD} Outing`, "recorded");
  const id = await importCollection(request, `${WORD} Atlas`);
  const { tracks } = await (await request.get(`/api/collections/${id}`)).json();
  const linked = await request.post("/api/links", {
    data: { trip_ids: [day], collection_track_ids: [tracks[0].id] },
  });
  expect(linked.status()).toBe(200);

  await page.goto(`/app/trips/${day}`);
  const lines = page.locator("#track-map path.leaflet-interactive");
  await expect(lines).toHaveCount(1);
  await expect(page.locator("#track-map .leaflet-map-pane")).not.toHaveClass(/leaflet-zoom-anim/);
  const view = () =>
    page.evaluate(() => {
      const map = window.tripArchiveWidgets["track-map"];
      const { lat, lng } = map.getCenter();
      return { zoom: map.getZoom(), lat: lat.toFixed(5), lng: lng.toFixed(5) };
    });
  const before = await view();

  // By its name: the suite's other trips on the same walk may be proposed
  // as links too (US-103), each with a checkbox of its own.
  const stage = page.getByRole("checkbox", { name: `Show ${tracks[0].name} on the map` });
  await stage.check();
  await expect(lines).toHaveCount(2);
  await expect(page.locator("#track-map .leaflet-map-pane")).not.toHaveClass(/leaflet-zoom-anim/);
  expect(await view()).toEqual(before);

  await stage.uncheck();
  await expect(lines).toHaveCount(1);
  expect(await view()).toEqual(before);
});
