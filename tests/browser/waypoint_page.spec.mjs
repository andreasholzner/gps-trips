// US-98: a collection's waypoints on its page and each on a page of its
// own, driven as the owner drives them (ADR-0012's browser layer).
//
// Scope rule, from the 2026-08-26b amendment: both exemptions apply. **Real
// user events** — clicking through, filling and submitting the form,
// choosing a symbol, tapping the placing map, confirming a delete.
// **JS-interop rendering** — the maps are Leaflet's. What the pages, the
// form and the chooser say is asserted in `crates/ui-dioxus` (`waypoint`,
// `collection`), and what the archive answers in
// `tests/it/us98_waypoint_pages.rs`.
import { expect, signIn, test } from "./session.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/// One short tour near Innsbruck; "Start" and a waypoint without a name
/// belong to its track.
const COLLECTION_SINGLE_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/collection_single.gpx", import.meta.url)),
);

/// Every collection these tests create, taken away again afterwards.
const created = [];

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  for (const id of created.splice(0)) {
    await request.delete(`/api/collections/${id}`);
  }
});

/// A collection of its own — names are unique — and its waypoints.
async function aTour(request) {
  const name = `Tour ${Math.random().toString(36).slice(2, 8)}`;
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: { name: "tour.gpx", mimeType: "application/gpx+xml", buffer: COLLECTION_SINGLE_GPX },
      name,
      activity_type: "hiking",
    },
  });
  expect(response.status(), "importing a collection").toBe(201);
  const { id } = await response.json();
  created.push(id);
  const waypoints = await (await request.get(`/api/collections/${id}/waypoints`)).json();
  return { id, waypoints };
}

const stored = async (request, id) =>
  (await (await request.get(`/api/waypoints/${id}`)).json()).waypoint;

test("a waypoint is reached from its collection's list and corrected in the form (US-98)", async ({
  page,
  request,
}) => {
  const tour = await aTour(request);
  const [start] = tour.waypoints;
  await page.goto(`/app/collections/${tour.id}`);

  const rows = page.locator("#collection-waypoints tbody tr");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(1)).toContainText("Unnamed waypoint");
  await rows.getByRole("link", { name: "Start" }).click();
  await expect(page).toHaveURL(new RegExp(`/app/waypoints/${start.id}$`));
  await expect(page.locator("#waypoint-name")).toHaveText("Start");
  // The waypoint and its track's line on the map.
  await expect(page.locator("#overview-map .leaflet-marker-icon")).toHaveCount(1);
  await expect(page.locator("#overview-map path.leaflet-interactive")).toHaveCount(1);

  await page.locator("#edit-waypoint").click();
  const form = page.getByRole("dialog", { name: "Edit waypoint" });
  await form.locator("#waypoint-edit-name").fill("Hungerburg");
  await form.locator("#waypoint-edit-ele").fill("");
  await form.locator("label.symbol-option", { hasText: /^Summit$/ }).click();
  await form.getByRole("button", { name: "Save" }).click();

  await expect(form).toHaveCount(0);
  await expect(page.locator("#waypoint-name")).toHaveText("Hungerburg");
  await expect(page.locator("#waypoint-symbol")).toHaveText("Summit");
  await expect(page.locator("#waypoint-ele")).toHaveText("—");
  const edited = await stored(request, start.id);
  expect([edited.name, edited.symbol, edited.ele]).toEqual(["Hungerburg", "Summit", null]);

  // Back on the collection, the list says so.
  await page.locator("#collection-of-waypoint a").click();
  await expect(rows.first()).toContainText("Hungerburg");
});

test("a waypoint is moved on the placing map, and cancelling changes nothing (US-98)", async ({
  page,
  request,
}) => {
  const tour = await aTour(request);
  const [start] = tour.waypoints;
  await page.goto(`/app/waypoints/${start.id}`);
  await expect(page.locator("#overview-map.leaflet-container")).toBeVisible();

  const tap = async (placing) => {
    const map = await placing.locator("#place-map").boundingBox();
    await page.mouse.click(map.x + map.width * 0.3, map.y + map.height * 0.3);
    await expect(placing.locator("#place-map .place-picked")).toHaveCount(1);
  };

  await page.locator("#move-waypoint").click();
  let placing = page.getByRole("dialog", { name: "Move Start" });
  // No warning: no position of a waypoint is automatic.
  await expect(placing.locator("#place-warning")).toHaveCount(0);
  await expect(placing.locator("#place-map .place-current")).toHaveCount(1);
  await expect(placing.getByRole("button", { name: "Save" })).toBeDisabled();
  await tap(placing);
  await placing.getByRole("button", { name: "Cancel" }).click();
  await expect(placing).toHaveCount(0);
  let now = await stored(request, start.id);
  expect([now.lat, now.lon]).toEqual([start.lat, start.lon]);

  await page.locator("#move-waypoint").click();
  placing = page.getByRole("dialog", { name: "Move Start" });
  await tap(placing);
  await placing.getByRole("button", { name: "Save" }).click();

  await expect(placing).toHaveCount(0);
  now = await stored(request, start.id);
  expect([now.lat, now.lon]).not.toEqual([start.lat, start.lon]);
  await page.reload();
  await expect(page.locator("#waypoint-position")).toContainText(now.lat.toFixed(5));
});

test("a waypoint is deleted after a confirmation, and the screen goes back to its collection (US-98)", async ({
  page,
  request,
}) => {
  const tour = await aTour(request);
  const [start] = tour.waypoints;
  await page.goto(`/app/waypoints/${start.id}`);

  await page.locator("#delete-waypoint").click();
  const confirmation = page.locator(".confirm");
  await expect(confirmation).toContainText("Delete the waypoint “Start”?");
  await confirmation.getByRole("button", { name: "Delete it" }).click();

  await expect(page).toHaveURL(new RegExp(`/app/collections/${tour.id}\\??$`));
  await expect(page.locator("#collection-waypoints tbody tr")).toHaveCount(1);
  expect((await request.get(`/api/waypoints/${start.id}`)).status()).toBe(404);
});
