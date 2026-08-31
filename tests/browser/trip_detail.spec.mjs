// The trip-detail screen driven as the owner drives it (US-42, ADR-0012's
// browser layer).
//
// Scope rule, from the 2026-08-26b amendment: this file covers only what the
// host-target tests structurally cannot — real user events, and the rendering
// that `document::eval` does. `dioxus-ssr` dispatches no events and runs no
// JavaScript, so the map, the chart and the photo markers draw into nothing
// there. Each test below names which of the two exemptions it needs.
//
// These assertions are the ones that moved off the server-rendered detail
// page when US-42 deleted it: coverage transferred, it did not evaporate.
import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const SAMPLE_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/sample.gpx", import.meta.url)),
);

/// A geotagged JPEG (US-3): the fixture the server's own tests use, so the
/// EXIF path here is the real one.
const GEOTAGGED_JPEG = readFileSync(
  fileURLToPath(new URL("../fixtures/geotagged.jpg", import.meta.url)),
);

/// Import a trip through the real API and return its id, from the redirect —
/// which US-42 repointed at the SPA's own screen.
async function importTrip(request, name) {
  const response = await request.post("/api/import", {
    maxRedirects: 0,
    multipart: {
      gpx: { name: "track.gpx", mimeType: "application/gpx+xml", buffer: SAMPLE_GPX },
      name,
    },
  });
  expect(response.status(), `importing ${name}`).toBe(303);
  return Number(response.headers()["location"].replace("/app/trips/", ""));
}

/// Every trip these tests import, so they can be taken away again. The suite
/// shares one archive: a trip left behind here is a row the list spec did not
/// seed and does not expect.
const created = [];

/// A trip of this test's own, so one test's edits cannot reach another's.
async function ownTrip(request, label) {
  const id = await importTrip(request, `${label} ${Math.random().toString(36).slice(2, 8)}`);
  created.push(id);
  return id;
}

test.afterAll(async ({ request }) => {
  // Best effort, and 404 is a fine outcome — the delete test removes its own.
  for (const id of created.splice(0)) {
    await request.delete(`/api/trips/${id}`);
  }
});

// US-7: "shows the track on an OSM map, an elevation profile, and a photo
// gallery with map markers." Every one of those is drawn by a JS library
// through `document::eval` — this layer's second exemption, and the only
// place any of it is observable.
test("the track, the elevation profile and a photo marker are drawn (US-7, US-3)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Drawn Trip");
  const uploaded = await request.post(`/api/trips/${id}/photos`, {
    multipart: {
      photos: { name: "geotagged.jpg", mimeType: "image/jpeg", buffer: GEOTAGGED_JPEG },
    },
  });
  expect(uploaded.status(), "seeding a geotagged photo").toBe(204);

  await page.goto(`/app/trips/${id}`);

  // Leaflet took the container over and drew the track on tiles it fetched.
  // The two overlays are told apart by their colours, which is all the DOM
  // says about them: a line is a line and a marker is a circle.
  await expect(page.locator("#track-map.leaflet-container")).toBeVisible();
  await expect(page.locator("#track-map img.leaflet-tile").first()).toBeVisible();
  await expect(page.locator('#track-map path[stroke="#3367d6"]')).toBeVisible();

  // uPlot drew the elevation profile into its own container.
  await expect(page.locator("#elevation canvas")).toBeVisible();

  // US-3: the geotagged photo is on the map, and in the gallery. A circle
  // marker drawn by Leaflet rather than fetched — its default pin is an image
  // file the bundle deliberately does not ship.
  await expect(page.locator('#track-map path[fill="#d6336c"]')).toBeVisible();
  await expect(page.getByRole("img", { name: "geotagged.jpg" })).toBeVisible();
});

// US-15: the edit form is opened, typed into and submitted — three real
// events, none of which `dioxus-ssr` can dispatch.
test("editing the name and activity saves them (US-15)", async ({ page, request }) => {
  const id = await ownTrip(request, "Edited Trip");
  await page.goto(`/app/trips/${id}`);

  await page.getByRole("button", { name: "Edit name / activity" }).click();
  await page.getByLabel("Name").fill("Renamed By Hand");
  await page.getByLabel("Activity").selectOption("cycling");
  await page.getByRole("button", { name: "Save" }).click();

  // The screen re-reads the trip rather than trusting what was typed.
  await expect(page.locator("#trip-name")).toHaveText("Renamed By Hand");
  await expect(page.locator("#trip-activity")).toHaveText("Cycling");

  // And it is the archive that changed, not just the screen.
  const trip = await (await request.get(`/api/trips/${id}`)).json();
  expect(trip.name).toBe("Renamed By Hand");
  expect(trip.activity_type).toBe("cycling");
});

// US-33: "using a new tag creates the tag on-demand after confirmation."
// The confirmation is a real click on a control that only appears after
// another one.
test("a new tag is created only after it is confirmed (US-33)", async ({ page, request }) => {
  const id = await ownTrip(request, "Tagged Trip");
  const name = `winter-${Math.random().toString(36).slice(2, 8)}`;
  await page.goto(`/app/trips/${id}`);
  await expect(page.getByText("No tags yet.")).toBeVisible();

  await page.locator("#tag-input").fill(name);
  await page.getByRole("button", { name: "Add tag" }).click();

  // Nothing is created until the owner says so.
  await expect(page.getByText(`Create a new tag "${name}"?`)).toBeVisible();
  expect(await (await request.get(`/api/trips/${id}/tags`)).json()).toEqual([]);

  await page.getByRole("button", { name: "Create it" }).click();

  await expect(page.getByText(name, { exact: false })).toBeVisible();
  const tags = await (await request.get(`/api/trips/${id}/tags`)).json();
  expect(tags.map((tag) => tag.name)).toContain(name);
});

// US-2's other half from the SPA: choosing a file is a browser gesture, and
// clearing the picker afterwards is a DOM write no Rust state owns.
test("a photo added later appears in the gallery (US-2)", async ({ page, request }) => {
  const id = await ownTrip(request, "Photo Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.getByText("No photos yet.")).toBeVisible();

  await page.locator("#add-photos-input").setInputFiles({
    name: "added-later.jpg",
    mimeType: "image/jpeg",
    buffer: GEOTAGGED_JPEG,
  });
  await page.getByRole("button", { name: "Add photos" }).click();

  await expect(page.getByRole("img", { name: "added-later.jpg" })).toBeVisible();
  // The picker no longer names a file it has already uploaded, so the button
  // does not contradict it.
  await expect(page.locator("#add-photos-input")).toHaveValue("");
});

// US-9: deleting is armed, confirmed and then leaves the screen — a sequence
// of real clicks ending in a navigation.
test("deleting a trip asks first, then leads back to the list (US-9)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Doomed Trip");
  await page.goto(`/app/trips/${id}`);

  await page.getByRole("button", { name: "Delete trip" }).click();
  await expect(page.getByText("This cannot be undone")).toBeVisible();
  // Still there while the question stands.
  expect((await request.get(`/api/trips/${id}`)).status()).toBe(200);

  await page.getByRole("button", { name: "Delete it" }).click();

  await expect(page).toHaveURL(/\/app\/(\?|$)/);
  expect((await request.get(`/api/trips/${id}`)).status()).toBe(404);
});

// US-42 made the row a client-side route: following it must not reload the
// page, which is only observable by watching for a navigation.
test("a row leads into the detail screen without a page load (US-42)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Linked Trip");
  await page.goto("/app/");

  const name = await (await request.get(`/api/trips/${id}`)).json().then((t) => t.name);
  await page.evaluate(() => {
    window.__stillTheSameDocument = true;
  });
  await page.getByRole("link", { name }).click();

  await expect(page.locator("#trip-name")).toHaveText(name);
  await expect(page).toHaveURL(new RegExp(`/app/trips/${id}$`));
  expect(await page.evaluate(() => window.__stillTheSameDocument)).toBe(true);
});
