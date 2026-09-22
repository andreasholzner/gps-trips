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
import { GEOTAGGED_JPEG, ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

/// The chart's x range, and the band a drag draws across it. A zoom redraws
/// a canvas and the band is uPlot's own element, so the widget is the only
/// place either is observable from outside.
const xScale = (page) =>
  page.evaluate(() => {
    const chart = window.tripArchiveWidgets.elevation;
    return [chart.scales.x.min, chart.scales.x.max];
  });

const selectionWidth = (page) =>
  page.evaluate(() => {
    const band = document.querySelector("#elevation .u-select");
    return band ? Math.round(parseFloat(getComputedStyle(band).width)) : 0;
  });

// US-19: the archive is gated, so the browser needs a session before any of
// this can run. The seeding `request` fixture arrives with one already
// (`session.mjs`).
test.beforeEach(async ({ page }) => {
  await signIn(page.request);
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

  // A ring around the count: white all the way round, and the number in the
  // middle of it. Both were lost to CSS from outside this rule — Leaflet's
  // own `.leaflet-marker-icon` outranked the centring, and US-58's
  // neutralising rule outranked three sides of the border.
  const shape = await cluster.evaluate((el) => {
    const style = getComputedStyle(el);
    const box = el.getBoundingClientRect();
    const glyph = document.createRange();
    glyph.selectNodeContents(el);
    const count = glyph.getBoundingClientRect();
    return {
      borders: [
        style.borderTopWidth,
        style.borderRightWidth,
        style.borderBottomWidth,
        style.borderLeftWidth,
      ],
      offX: Math.abs((count.left + count.right) / 2 - (box.left + box.right) / 2),
      offY: Math.abs((count.top + count.bottom) / 2 - (box.top + box.bottom) / 2),
    };
  });
  expect(shape.borders, "a ring, not an edge").toEqual(["2px", "2px", "2px", "2px"]);
  expect(shape.offX, "the count sits in the middle across").toBeLessThanOrEqual(1.5);
  expect(shape.offY, "the count sits in the middle down").toBeLessThanOrEqual(1.5);

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

// US-59's mark against US-57's badges. A group badge is a real marker and a
// marker pane sits above the overlay pane every vector is drawn in, so the
// ring needs a pane of its own or it goes under the very marker it is next
// to. Stacking is only real once both libraries have drawn.
test("the hovered point is marked above every photo marker (US-59)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Stacked Trip");
  // Two photos at one place, so the map carries a group badge as well as the
  // track: a single photo's circle is a vector and never outranked the ring.
  for (const name of ["first.jpg", "second.jpg"]) {
    await request.post(`/api/trips/${id}/photos`, {
      multipart: { photos: { name, mimeType: "image/jpeg", buffer: GEOTAGGED_JPEG } },
    });
  }
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#track-map .photo-cluster")).toHaveCount(1);
  await page.locator("#elevation").scrollIntoViewIfNeeded();

  const chart = await page.locator("#elevation").boundingBox();
  await page.mouse.move(chart.x + chart.width / 2, chart.y + chart.height / 2);
  await expect(page.locator("#track-map .hover-mark")).toHaveCount(1);

  // The ring is in a pane of its own, and that pane is painted after the one
  // holding the badges.
  const stacking = await page.evaluate(() => {
    const ring = document.querySelector("#track-map .hover-mark");
    const pane = ring.closest(".leaflet-pane");
    const markers = document.querySelector("#track-map .leaflet-marker-pane");
    return {
      pane: pane.className,
      ringZ: Number(getComputedStyle(pane).zIndex),
      markerZ: Number(getComputedStyle(markers).zIndex),
    };
  });
  expect(stacking.pane, "a pane of its own, not the overlay pane").not.toContain(
    "leaflet-overlay-pane",
  );
  expect(stacking.ringZ).toBeGreaterThan(stacking.markerZ);
});

// Kept deliberately (US-59): with a mouse, dragging across the profile zooms
// into that range, and a double-click puts it back. Only the finger is spared
// the zoom, because a touchscreen has no double-click to undo it with.
test("a mouse drag still zooms the profile (US-59)", async ({ page, request }) => {
  const id = await ownTrip(request, "Zoomed Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#elevation canvas")).toBeVisible();
  await page.locator("#elevation").scrollIntoViewIfNeeded();

  const chart = await page.locator("#elevation").boundingBox();
  const y = chart.y + chart.height / 2;
  const whole = await xScale(page);

  await page.mouse.move(chart.x + chart.width * 0.2, y);
  await page.mouse.down();
  await page.mouse.move(chart.x + chart.width * 0.5, y, { steps: 5 });
  await page.mouse.up();

  await expect.poll(() => xScale(page)).not.toEqual(whole);
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
  expect(Math.round(drawn.width), "badge width").toBe(20);
  expect(Math.round(drawn.height), "badge height").toBe(20);
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
  // US-62: the field is not there until adding is asked for.
  await expect(page.locator("#tag-input")).toHaveCount(0);

  await page.getByRole("button", { name: "Add tag" }).click();
  await page.locator("#tag-input").fill(name);
  await page.getByRole("button", { name: "Add", exact: true }).click();

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
  // US-62: the form is behind a button of its own, closed until asked for.
  await expect(page.locator("#add-photos-input")).toHaveCount(0);
  await page.getByRole("button", { name: "Add photos" }).click();

  await page.locator("#add-photos-input").setInputFiles({
    name: "added-later.jpg",
    mimeType: "image/jpeg",
    buffer: GEOTAGGED_JPEG,
  });
  await page.getByRole("button", { name: "Upload" }).click();

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
    const whole = await xScale(page);

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

    const reading = await page.locator("#readout-distance").textContent();
    await touch.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });

    // Lifting the finger is not "the pointer left the chart": a touch pointer
    // is destroyed on lift, and clearing the reading then would make it flash
    // and vanish. It stays until the next gesture.
    await expect(page.locator("#readout-distance")).toHaveText(reading);
    await expect(page.locator("#track-map .hover-mark")).toHaveCount(1);

    // And reading the profile does not rescale it. A drag zooms with a mouse,
    // which the owner can undo by double-clicking; a finger has no
    // double-click, so a zoom it cannot undo is a trap rather than a feature.
    expect(await xScale(page)).toEqual(whole);
    // Nor does it leave the selection band the drag drew behind it.
    expect(await selectionWidth(page)).toBe(0);
  });

  test("a tap on the profile reads out that point and keeps it (US-59)", async ({
    page,
    request,
  }) => {
    const id = await ownTrip(request, "Tapped Trip");
    await page.goto(`/app/trips/${id}`);
    await expect(page.locator("#elevation canvas")).toBeVisible();
    await page.locator("#elevation").scrollIntoViewIfNeeded();
    const chart = await page.locator("#elevation").boundingBox();

    // A tap sends `pointerdown` and `pointerup` and no `pointermove` at all,
    // so uPlot — which only ever places the cursor on a move — would read
    // nothing from it.
    await page.touchscreen.tap(chart.x + chart.width * 0.45, chart.y + chart.height / 2);

    await expect(page.locator("#readout-distance")).toHaveText(/^\d+\.\d\d km$/);
    await expect(page.locator("#readout-elevation")).toHaveText(/^\d+ m$/);
    await expect(page.locator("#track-map .hover-mark")).toHaveCount(1);

    // Still there once the finger is long gone.
    const reading = await page.locator("#readout-distance").textContent();
    await page.waitForTimeout(500);
    await expect(page.locator("#readout-distance")).toHaveText(reading);

    // A second tap elsewhere reads that point instead.
    await page.touchscreen.tap(chart.x + chart.width * 0.8, chart.y + chart.height / 2);
    await expect(page.locator("#readout-distance")).not.toHaveText(reading);
  });
});
