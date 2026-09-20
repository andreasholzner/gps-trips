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
import { expect, signIn, test } from "./session.mjs";
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

// US-19: the archive is gated, so the browser needs a session before any of
// this can run. The seeding `request` fixture arrives with one already
// (`session.mjs`).
test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

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

// US-57: photos taken at the same place share one marker whose popup holds
// all of them. Which photos are grouped is decided in Rust and unit-tested
// there; what a click on the marker then shows is this layer's second
// exemption — the popup exists only once Leaflet has drawn it.
test("photos taken at the same place share one marker that shows them all (US-57)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Crowded Trip");
  // The same geotagged fixture twice: two photos, one position — which is
  // what US-4's interpolation produces routinely by snapping several photos
  // onto a single track point.
  for (const name of ["first.jpg", "second.jpg"]) {
    const uploaded = await request.post(`/api/trips/${id}/photos`, {
      multipart: { photos: { name, mimeType: "image/jpeg", buffer: GEOTAGGED_JPEG } },
    });
    expect(uploaded.status(), `seeding ${name}`).toBe(204);
  }

  await page.goto(`/app/trips/${id}`);

  // One marker for the two of them, and it says so. Stacked circles would
  // leave only the topmost clickable — the bug this story is about.
  const cluster = page.locator("#track-map .photo-cluster");
  await expect(cluster).toHaveCount(1);
  await expect(cluster).toHaveText("2");

  await cluster.click();

  const popup = page.locator("#track-map .photo-popup");
  await expect(popup.getByText("2 photos here")).toBeVisible();
  await expect(popup.getByRole("img", { name: "first.jpg" })).toBeVisible();
  await expect(popup.getByRole("img", { name: "second.jpg" })).toBeVisible();
});

// US-59: hovering the profile reads out the point and marks it on the track.
// Both exemptions at once — a real pointer moving, and two JS widgets whose
// drawing is the thing under test. The index round-trips through Rust in
// between, which is what `track::hover_points` covers without a browser.
test("hovering the elevation profile reads out the point and marks it on the track (US-59)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Hovered Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#elevation canvas")).toBeVisible();

  // uPlot's own legend is gone: it is the control that hides the series, and
  // its marker square reads as a checkbox.
  await expect(page.locator("#elevation .u-legend")).toHaveCount(0);

  // Nothing is hovered yet, so the readout claims no position and the map
  // carries no mark.
  await expect(page.locator("#readout-distance")).toHaveText("—");
  await expect(page.locator("#readout-elevation")).toHaveText("—");
  await expect(page.locator("#track-map .hover-mark")).toHaveCount(0);

  // Onto the middle of the chart. `page.mouse` emits real pointer events,
  // which is what the rebound `cursor.bind` listens for — but it works in
  // viewport coordinates and does not scroll the way `locator.click()` does,
  // so the chart has to be brought into view or the move lands elsewhere
  // entirely (the same trap the region map's drag documents).
  await page.locator("#elevation").scrollIntoViewIfNeeded();
  const chart = await page.locator("#elevation").boundingBox();
  await page.mouse.move(chart.x + chart.width / 2, chart.y + chart.height / 2);

  await expect(page.locator("#readout-distance")).toHaveText(/^\d+\.\d\d km$/);
  await expect(page.locator("#readout-elevation")).toHaveText(/^\d+ m$/);
  // The index reached Rust, was resolved to a position, and came back to the
  // map as a mark on the track.
  await expect(page.locator("#track-map .hover-mark")).toHaveCount(1);

  // Further along the chart is a different point of the track.
  const first = await page.locator("#readout-distance").textContent();
  await page.mouse.move(chart.x + chart.width * 0.85, chart.y + chart.height / 2);
  await expect(page.locator("#readout-distance")).not.toHaveText(first);

  // Off the chart: the screen stops showing a point nothing is pointing at.
  await page.mouse.move(chart.x + chart.width / 2, chart.y - 80);
  await expect(page.locator("#readout-distance")).toHaveText("—");
  await expect(page.locator("#track-map .hover-mark")).toHaveCount(0);
});

// US-58: Pico's classless build styles every `[role=button]`, and Leaflet
// puts that role on its zoom buttons and on every keyboard-reachable marker.
// What that costs is a box size, which no host-target layer can see — this
// layer's first business is real rendering, and geometry is only real once a
// browser has laid it out.
test("the zoom control and the photo markers keep their own size (US-58)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Sized Trip");
  const uploaded = await request.post(`/api/trips/${id}/photos`, {
    multipart: {
      photos: { name: "geotagged.jpg", mimeType: "image/jpeg", buffer: GEOTAGGED_JPEG },
    },
  });
  expect(uploaded.status(), "seeding a geotagged photo").toBe(204);

  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#track-map.leaflet-container")).toBeVisible();

  // Both buttons are the 30x30 Leaflet draws them as, and both are inside
  // the bar: Pico's padding made them 42x32 and pushed the "−" out of it.
  const bar = await page.locator("#track-map .leaflet-control-zoom").boundingBox();
  for (const label of ["Zoom in", "Zoom out"]) {
    const button = await page.getByRole("button", { name: label }).boundingBox();
    expect(Math.round(button.width), `${label} width`).toBe(30);
    expect(Math.round(button.height), `${label} height`).toBe(30);
    expect(button.y + button.height, `${label} inside the bar`).toBeLessThanOrEqual(
      bar.y + bar.height + 1,
    );
  }

  // A marker carries the same role, so US-57's badge was stretched into an
  // ellipse by the same rule — and sat off the point it stands for.
  await request.post(`/api/trips/${id}/photos`, {
    multipart: { photos: { name: "second.jpg", mimeType: "image/jpeg", buffer: GEOTAGGED_JPEG } },
  });
  await page.reload();
  const badge = page.locator("#track-map .photo-cluster");
  await expect(badge).toHaveText("2");
  const drawn = await badge.boundingBox();
  expect(Math.round(drawn.width), "badge width").toBe(26);
  expect(Math.round(drawn.height), "badge height").toBe(26);
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

// US-59 with a finger. uPlot v1.6.31 binds mouse events by name, which a
// touchscreen never sends, and the page would otherwise take a drag along
// the chart as a scroll — so the pointer rebinding and `touch-action` are
// only real in a browser with a touchscreen, which is its own context.
test.describe("with a touchscreen", () => {
  test.use({ hasTouch: true });

  test("a finger drag along the profile moves the readout and marks the track (US-59)", async ({
    page,
    request,
  }) => {
    const id = await ownTrip(request, "Touched Trip");
    await page.goto(`/app/trips/${id}`);
    await expect(page.locator("#elevation canvas")).toBeVisible();
    await page.locator("#elevation").scrollIntoViewIfNeeded();

    const chart = await page.locator("#elevation").boundingBox();
    const y = chart.y + chart.height / 2;
    const scrolledTo = await page.evaluate(() => window.scrollY);

    // Playwright's touchscreen can tap but not drag, and a tap sends no
    // `pointermove` at all — the gesture this story is about. So the drag is
    // dispatched as real touch input.
    const touch = await page.context().newCDPSession(page);
    const at = (fraction) => ({ x: chart.x + chart.width * fraction, y });
    await touch.send("Input.dispatchTouchEvent", {
      type: "touchStart",
      touchPoints: [at(0.3)],
    });
    for (const fraction of [0.4, 0.5, 0.6]) {
      await touch.send("Input.dispatchTouchEvent", {
        type: "touchMove",
        touchPoints: [at(fraction)],
      });
    }

    await expect(page.locator("#readout-distance")).toHaveText(/^\d+\.\d\d km$/);
    await expect(page.locator("#track-map .hover-mark")).toHaveCount(1);
    // `touch-action: pan-y` claims the horizontal gesture: the chart reads it
    // instead of the page scrolling out from under the finger.
    expect(await page.evaluate(() => window.scrollY)).toBe(scrolledTo);

    await touch.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  });
});
