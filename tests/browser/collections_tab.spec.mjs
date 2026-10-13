// The trip list's collections (US-87), listed with the trips by kind (US-90),
// and their selection (US-94, US-100), driven as the owner drives them.
//
// Scope rule, from ADR-0012's 2026-08-26b amendment: both exemptions apply.
// **Real user events** — toggling kinds, folding a group out, ticking rows,
// typing a search, reloading, panning, clicking a row or a line. **JS-interop
// rendering** — the map is Leaflet's. What the list renders is asserted in
// `crates/ui-dioxus` (`list/collections_tests.rs`, `list/kinds_tests.rs`,
// `list_table.rs`, `selection_bar.rs`, `filter_bar.rs`, `region.rs`), and what
// the archive answers in `tests/it/us87_collections_tab.rs` and
// `tests/it/us90_trips_and_collections.rs`.
import { expect, signIn, test } from "./session.mjs";
import { ownTrips } from "./trips.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const fixture = (name) =>
  readFileSync(fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url)));

/// One short tour near Innsbruck, whose two waypoints belong to its track.
const COLLECTION_SINGLE_GPX = fixture("collection_single.gpx");

/// Two waypoints, the San Bernardino and Albula passes, and no track.
const WAYPOINTS_ONLY_GPX = fixture("waypoints_only.gpx");

/// A word no other spec's names hold, so a search for it lists this file's
/// collections alone: the suite shares one archive.
const TAG = `book${Math.random().toString(36).slice(2, 8)}`;

/// Every collection these tests create, taken away again afterwards.
const created = [];

const rows = (page) => page.locator("table tbody tr");
const checked = (page) => page.locator("table tbody input[type=checkbox]:checked");
const trip = ownTrips(test);
const lines = (page) => page.locator("#region-map .trip-line");

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
  expect(response.status(), "importing a collection").toBe(201);
  const { id } = await response.json();
  created.push(id);
  return id;
}

/// Drag the region map sideways from `from` to `to`, as fractions of its
/// width: a real pan. The map is brought into view first, since
/// `page.mouse` works in viewport coordinates.
async function pan(page, from, to) {
  const map = page.locator("#region-map");
  await map.scrollIntoViewIfNeeded();
  const box = await map.boundingBox();
  await page.mouse.move(box.x + box.width * from, box.y + box.height * 0.5);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * to, box.y + box.height * 0.5, { steps: 8 });
  await page.mouse.up();
}

/// A point on the line itself, halfway along it, in viewport coordinates.
const onTheLine = (line) =>
  line.evaluate((path) => {
    const at = path
      .getPointAtLength(path.getTotalLength() / 2)
      .matrixTransform(path.getScreenCTM());
    return { x: at.x, y: at.y };
  });

test("collections are a kind in the URL, the search narrows them, and the trip filters wait", async ({
  page,
  request,
}) => {
  const tour = await importCollection(request, COLLECTION_SINGLE_GPX, {
    name: `${TAG} Nordkette`,
    activity_type: "hiking",
  });
  await importCollection(request, WAYPOINTS_ONLY_GPX, { name: `${TAG} Passes` });
  // A trip filter, set before switching trips off: hidden then, kept for after.
  await page.goto(`/app/?from=2000-01-01&q=${TAG}`);
  await expect(page.getByText("More filters")).toBeVisible();

  await page.getByRole("button", { name: "Collections" }).click();
  await expect(page).toHaveURL(/kind=recorded%2Ccollections/);
  await page.getByRole("button", { name: "Recorded" }).click();
  await expect(page).toHaveURL(/kind=collections/);
  await expect(rows(page)).toHaveCount(2);
  // The activity narrows collections too (US-90); the rest is about trips.
  await expect(page.locator("#filter-activity")).toBeVisible();
  await expect(page.getByText("More filters")).toHaveCount(0);
  await expect(page).toHaveURL(/from=2000-01-01/);
  // The last kind on stays on.
  await expect(page.getByRole("button", { name: "Collections" })).toBeDisabled();

  // A reload starts on collections alone.
  await page.reload();
  await expect(rows(page)).toHaveCount(2);
  await expect(page.locator(".trip-counts")).toHaveText(/^2 (of \d+ )?collections$/);

  await page.getByRole("searchbox").fill(`${TAG} nord`);
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page)).toContainText("Nordkette");

  // With trips back on, the date filter is where the owner left it.
  await page.getByRole("button", { name: "Recorded" }).click();
  await expect(page.getByText("More filters")).toBeVisible();
  await expect(page).toHaveURL(/from=2000-01-01/);

  // And a group row opens its collection's page.
  await rows(page)
    .getByRole("link", { name: `${TAG} Nordkette` })
    .click();
  await expect(page).toHaveURL(new RegExp(`/app/collections/${tour}$`));
});

test("zoomed in, a collection's tracks are lines, and a click opens the track", async ({
  page,
  request,
}) => {
  const id = await importCollection(request, COLLECTION_SINGLE_GPX, { name: `${TAG} Lines` });
  const [track] = (await (await request.get(`/api/collections/${id}`)).json()).tracks;
  await page.goto(`/app/?kind=collections&q=${encodeURIComponent(`${TAG} Lines`)}`);

  // Fitted to one short tour, the map is zoomed in past the threshold.
  await expect(lines(page)).toHaveCount(1);
  // Its two waypoints show with it, and the legend names them (US-90).
  await expect(
    page.locator("#region-map .waypoint-mark, #region-map .waypoint-symbol"),
  ).toHaveCount(2);
  await expect(page.locator(".map-legend")).toContainText("Waypoint");
  await page.locator("#region-map").scrollIntoViewIfNeeded();
  const at = await onTheLine(lines(page));
  await page.mouse.move(at.x, at.y);
  await expect(page.locator("#region-map .leaflet-tooltip")).toHaveText(track.name);

  await page.mouse.click(at.x, at.y);
  await expect(page).toHaveURL(new RegExp(`/app/collection-tracks/${track.id}$`));
});

test("the region narrows the collections by their waypoints too, and the fit takes them in", async ({
  page,
  request,
}) => {
  await importCollection(request, WAYPOINTS_ONLY_GPX, { name: `${TAG} Region` });
  await page.goto(`/app/?kind=collections&q=${encodeURIComponent(`${TAG} Region`)}`);
  await expect(rows(page)).toHaveCount(1);
  const fit = page.locator("#region-fit");
  await expect(fit).toHaveText("Fit to items");

  // Fitted to the waypoints, the map's view holds them: still listed.
  const follow = page.locator("#region-follow");
  await follow.check();
  await expect(page).toHaveURL(/[?&]bbox=/);
  await expect(rows(page)).toHaveCount(1);

  // Panned off them, the list follows the view.
  await pan(page, 0.05, 0.95);
  await pan(page, 0.05, 0.95);
  await expect(page.getByText("No collections match your filters.")).toBeVisible();

  // Off again, the collection is listed while the map stays where it is.
  // Fitting to the collections brings the waypoints back into view, so
  // filtering to the map once more keeps it listed.
  await follow.uncheck();
  await expect(rows(page)).toHaveCount(1);
  await fit.click();
  await follow.check();
  await expect(page).toHaveURL(/[?&]bbox=/);
  await expect(rows(page)).toHaveCount(1);
  await expect(page.getByText("No collections match your filters.")).toHaveCount(0);
});

test("trips, collections and their tracks are selected together, and the actions follow what is (US-90)", async ({
  page,
  request,
}) => {
  // A word of this test's own: the file's other tests list theirs under TAG.
  const word = `${TAG}mixed`;
  await trip(request, `${word} Walk`);
  await importCollection(request, COLLECTION_SINGLE_GPX, { name: `${word} Book` });
  await page.goto(`/app/?kind=recorded%2Ccollections&q=${word}`);
  await expect(rows(page)).toHaveCount(2);
  const tag = page.getByRole("button", { name: "Tag…" });
  const share = page.getByRole("button", { name: "Share…" });
  const walk = rows(page).filter({ hasText: `${word} Walk` });
  const book = rows(page).filter({ hasText: `${word} Book` });

  await walk.getByRole("checkbox").check();
  await expect(page.getByText("1 selected: 1 trip")).toBeVisible();
  await expect(tag).toBeEnabled();

  // A trip and a collection together: no action takes both.
  await book.getByRole("checkbox").check();
  await expect(page.getByText("2 selected: 1 trip · 1 collection")).toBeVisible();
  await expect(tag).toBeDisabled();
  await expect(share).toBeDisabled();

  // The collection alone is tagged as collections are (US-100).
  await walk.getByRole("checkbox").uncheck();
  await tag.click();
  await expect(page.getByText("Tag selected collections")).toBeVisible();

  // Folded out, the group lists its track, which is selected on its own.
  await book.getByRole("button", { name: "Show its tracks" }).click();
  await expect(rows(page)).toHaveCount(3);
  await rows(page).nth(2).getByRole("checkbox").check();
  await expect(page.getByText("2 selected: 1 collection · 1 collection track")).toBeVisible();
  await expect(page.getByText("Tag selected collections")).toHaveCount(0);

  // Switching collections off drops what they listed from the selection.
  await page.getByRole("button", { name: "Collections" }).click();
  await expect(rows(page)).toHaveCount(1);
  await expect(page.locator(".selection-bar")).toHaveCount(0);
});

test("waypoints that cannot be read hold no line back (US-90)", async ({ page, request }) => {
  await importCollection(request, COLLECTION_SINGLE_GPX, { name: `${TAG} Unread` });
  await page.route("**/api/collection-waypoints?*", (route) => route.abort());

  await page.goto(`/app/?kind=collections&q=${encodeURIComponent(`${TAG} Unread`)}`);

  // Fitted to one short tour, the map is zoomed in: its line is drawn.
  await expect(lines(page)).toHaveCount(1);
  await expect(
    page.locator("#region-map .waypoint-mark, #region-map .waypoint-symbol"),
  ).toHaveCount(0);
});
