// US-62 driven as the owner drives it: the trip's page at a glance, and a
// photo looked at properly (ADR-0012's browser layer).
//
// Scope rule, from the 2026-08-26b amendment: only what the host-target tests
// structurally cannot reach — real user events, what `document::eval` draws,
// and layout, which only a browser computes. What the screen renders is
// asserted in `crates/ui-dioxus`.
import { expect, signIn, test } from "./session.mjs";
import { addPhotos, GEOTAGGED_JPEG, ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// Where each stats pair sits, as `[left, top]`.
const pairPositions = (page) =>
  page.locator("dl.stats > div").evaluateAll((pairs) =>
    pairs.map((pair) => {
      const box = pair.getBoundingClientRect();
      return [Math.round(box.left), Math.round(box.top)];
    }),
  );

// Layout: five pairs in one row from US-60's breakpoint up.
test("on a wide screen the numbers sit in one row (US-62)", async ({ page, request }) => {
  const id = await ownTrip(request, "Wide Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#trip-activity")).toBeVisible();

  const pairs = await pairPositions(page);
  expect(pairs).toHaveLength(5);
  expect(new Set(pairs.map(([, top]) => top)).size, "one row").toBe(1);
  // Label over value, not beside it.
  const [label, value] = await page
    .locator("dl.stats > div")
    .first()
    .evaluate((pair) => [...pair.children].map((el) => el.getBoundingClientRect().top));
  expect(value).toBeGreaterThan(label);
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("the numbers are a two-column grid (US-62)", async ({ page, request }) => {
    const id = await ownTrip(request, "Narrow Trip");
    await page.goto(`/app/trips/${id}`);
    await expect(page.locator("#trip-activity")).toBeVisible();

    const [first, second, third] = await pairPositions(page);
    expect(second[1], "the first two share a row").toBe(first[1]);
    expect(third[1], "the third starts the next").toBeGreaterThan(first[1]);
    expect(third[0], "in the first column").toBe(first[0]);
  });
});

// A real pointer over the chart; the time comes back through Rust.
test("hovering the profile reads out the time, with its offset (US-62)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Timed Trip");
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#elevation canvas")).toBeVisible();
  await expect(page.locator("#readout-time")).toHaveText("—");

  await page.locator("#elevation").scrollIntoViewIfNeeded();
  const chart = await page.locator("#elevation").boundingBox();
  await page.mouse.move(chart.x + chart.width / 2, chart.y + chart.height / 2);

  // SAMPLE_GPX is a June morning in Oslo, 08:00–09:00 UTC: 10:00–11:00 there,
  // and never across midnight, so a clock time without a date.
  await expect(page.locator("#readout-time")).toHaveText(/^1[01]:\d\d \(\+02:00\)$/);
});

// The edit form over the screen: a real click, a key, and a page that holds
// still behind it.
test("editing opens over the screen and Escape closes it (US-62)", async ({ page, request }) => {
  const id = await ownTrip(request, "Overlaid Trip");
  await page.goto(`/app/trips/${id}`);

  await page.getByRole("button", { name: "Edit name / activity" }).click();
  const dialog = page.getByRole("dialog", { name: "Edit trip" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel("Name")).toBeVisible();
  await expect(page.locator("body")).toHaveClass(/overlay-open/);
  expect(await page.evaluate(() => getComputedStyle(document.body).overflow)).toBe("hidden");

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.locator("body")).not.toHaveClass(/overlay-open/);

  // And its own Cancel does the same.
  await page.getByRole("button", { name: "Edit name / activity" }).click();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toHaveCount(0);
});

// ── The viewer ──────────────────────────────────────────────────────────────

/// A photo with no EXIF: in the gallery, but nowhere on the map.
const UNPLACED_JPEG = Buffer.from("\xFF\xD8\xFF-no-exif", "latin1");

/// A trip with two geotagged photos — one marker between them — and one the
/// map cannot place, so the gallery's set and the marker's differ.
async function photographedTrip(request, label) {
  const id = await ownTrip(request, label);
  await addPhotos(request, id, [
    ["first.jpg", GEOTAGGED_JPEG],
    ["second.jpg", GEOTAGGED_JPEG],
    ["unplaced.jpg", UNPLACED_JPEG],
  ]);
  return id;
}

// Real clicks and keys, over a page that holds still.
test("a thumbnail opens the photo, and the gallery is browsed from it (US-62)", async ({
  page,
  request,
}) => {
  const id = await photographedTrip(request, "Viewed Trip");
  await page.goto(`/app/trips/${id}`);

  await page.getByTitle("Open first.jpg").click();
  const viewer = page.getByRole("dialog", { name: "Photo viewer" });
  await expect(viewer).toBeVisible();
  // At the size the archive holds it, not the thumbnail.
  const photo = viewer.getByRole("img", { name: "first.jpg" });
  await expect(photo).toBeVisible();
  expect(await photo.getAttribute("src")).not.toContain("thumbs/");
  await expect(viewer.locator("#viewer-place")).toHaveText("1 / 3");
  expect(await page.evaluate(() => getComputedStyle(document.body).overflow)).toBe("hidden");

  // The ends do not wrap.
  await expect(viewer.getByRole("button", { name: "‹ Previous" })).toBeDisabled();
  await page.keyboard.press("ArrowLeft");
  await expect(viewer.locator("#viewer-place")).toHaveText("1 / 3");

  await page.keyboard.press("ArrowRight");
  await expect(viewer.getByRole("img", { name: "second.jpg" })).toBeVisible();
  await viewer.getByRole("button", { name: "Next ›" }).click();
  await expect(viewer.locator("#viewer-place")).toHaveText("3 / 3");
  await expect(viewer.getByRole("button", { name: "Next ›" })).toBeDisabled();
  await page.keyboard.press("ArrowRight");
  await expect(viewer.locator("#viewer-place")).toHaveText("3 / 3");

  await page.keyboard.press("ArrowLeft");
  await expect(viewer.locator("#viewer-place")).toHaveText("2 / 3");

  await page.keyboard.press("Escape");
  await expect(viewer).toHaveCount(0);
  await expect(page.locator("body")).not.toHaveClass(/overlay-open/);

  await page.getByTitle("Open second.jpg").click();
  await expect(viewer.locator("#viewer-place")).toHaveText("2 / 3");
  await viewer.getByRole("button", { name: "Close" }).click();
  await expect(viewer).toHaveCount(0);
});

// The popup is drawn by Leaflet; the tap crosses back into Rust over the
// map's channel (ADR-0025), which only a browser runs.
test("a photo tapped in a marker's popup browses only that marker's photos (US-62)", async ({
  page,
  request,
}) => {
  const id = await photographedTrip(request, "Popped Trip");
  await page.goto(`/app/trips/${id}`);

  await page.locator("#track-map .photo-cluster").click();
  await page.locator("#track-map .photo-popup").getByTitle("Open second.jpg").click();

  const viewer = page.getByRole("dialog", { name: "Photo viewer" });
  await expect(viewer.getByRole("img", { name: "second.jpg" })).toBeVisible();
  // The marker's two, not the gallery's three.
  await expect(viewer.locator("#viewer-place")).toHaveText("2 / 2");
  await expect(viewer.getByRole("button", { name: "Next ›" })).toBeDisabled();
});

test.describe("with a touchscreen", () => {
  test.use({ hasTouch: true, viewport: { width: 390, height: 844 } });

  // A real finger's drag, which the viewer has to claim from the browser.
  test("a swipe steps through the photos (US-62)", async ({ page, request }) => {
    const id = await photographedTrip(request, "Swiped Trip");
    await page.goto(`/app/trips/${id}`);
    await page.getByTitle("Open first.jpg").tap();
    const viewer = page.getByRole("dialog", { name: "Photo viewer" });
    await expect(viewer.locator("#viewer-place")).toHaveText("1 / 3");

    // Playwright's touchscreen can tap but not drag, so the drag is
    // dispatched as real touch input.
    const box = await viewer.locator(".viewer-photo img").boundingBox();
    const y = box.y + box.height / 2;
    const touch = await page.context().newCDPSession(page);
    const drag = async (fromX, toX) => {
      await touch.send("Input.dispatchTouchEvent", {
        type: "touchStart",
        touchPoints: [{ x: fromX, y }],
      });
      for (const f of [0.25, 0.5, 0.75, 1]) {
        await touch.send("Input.dispatchTouchEvent", {
          type: "touchMove",
          touchPoints: [{ x: fromX + (toX - fromX) * f, y }],
        });
      }
      await touch.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    };

    await drag(300, 100);
    await expect(viewer.locator("#viewer-place")).toHaveText("2 / 3");
    await drag(100, 300);
    await expect(viewer.locator("#viewer-place")).toHaveText("1 / 3");
    // A short drag is not a swipe.
    await drag(200, 180);
    await expect(viewer.locator("#viewer-place")).toHaveText("1 / 3");
  });
});

// Hover is a real pointer state, and the colour it ends in is Pico's custom
// properties resolved by a browser.
test("the quiet controls stay readable under the pointer (US-62)", async ({ page, request }) => {
  const id = await ownTrip(request, "Hovered Controls");
  await page.goto(`/app/trips/${id}`);

  for (const name of ["Add tag", "Edit name / activity", "Add photos"]) {
    const button = page.getByRole("button", { name });
    await button.hover();
    const pageText = await page.evaluate(() => getComputedStyle(document.body).color);
    // Polled: Pico eases the colour in over 0.2s.
    await expect
      .poll(() => button.evaluate((el) => getComputedStyle(el).color), {
        message: `${name} under the pointer`,
      })
      .toBe(pageText);
  }
});

// ── Placing a photo by hand (US-30) ─────────────────────────────────────────

/// Where the track map's photo markers are, as `[lat, lon]` — drawn by
/// Leaflet, so only its own layers can say.
const markerPositions = (page) =>
  page.evaluate(() =>
    window.tripArchiveWidgets["track-map"].photoMarkers
      .getLayers()
      .map((marker) => {
        const at = marker.getLatLng();
        return [at.lat, at.lng];
      }),
  );

/// The stored photo named `name`.
async function storedPhoto(request, id, name) {
  const photos = await (await request.get(`/api/trips/${id}/photos`)).json();
  return photos.find((photo) => photo.original_name === name);
}

// Real clicks on a map `document::eval` drew — both exemptions.
test("a geotagged photo is moved by hand, after a warning (US-30)", async ({ page, request }) => {
  const id = await ownTrip(request, "Placed Trip");
  await addPhotos(request, id, [["moved.jpg", GEOTAGGED_JPEG]]);
  const before = await storedPhoto(request, id, "moved.jpg");
  await page.goto(`/app/trips/${id}`);

  await page.getByTitle("Open moved.jpg").click();
  await page.getByRole("button", { name: "Place on map" }).click();
  const placing = page.getByRole("dialog", { name: "Place moved.jpg" });
  await expect(placing).toBeVisible();
  // The viewer handed over to it rather than staying open underneath.
  await expect(page.getByRole("dialog", { name: "Photo viewer" })).toHaveCount(0);
  await expect(placing.locator("#place-warning")).toContainText("GPS");
  await expect(placing.locator("#place-map path[stroke=\"#3367d6\"]")).toBeVisible();
  await expect(placing.locator("#place-map .place-current")).toHaveCount(1);
  await expect(placing.getByRole("button", { name: "Save" })).toBeDisabled();

  const map = await placing.locator("#place-map").boundingBox();
  await page.mouse.click(map.x + map.width * 0.2, map.y + map.height * 0.25);
  await expect(placing.locator("#place-map .place-picked")).toHaveCount(1);
  await placing.getByRole("button", { name: "Save" }).click();

  await expect(placing).toHaveCount(0);
  // Handed from one overlay to the next and closed: the page scrolls again.
  await expect(page.locator("body")).not.toHaveClass(/overlay-open/);
  const after = await storedPhoto(request, id, "moved.jpg");
  expect(after.location_source).toBe("manual");
  expect([after.lat, after.lon]).not.toEqual([before.lat, before.lon]);
  // And the track map's marker went with it.
  await expect
    .poll(() => markerPositions(page))
    .toEqual([[after.lat, after.lon]]);
});

test("a photo the map could not place is placed without a warning (US-30)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Unplaced Trip");
  await addPhotos(request, id, [["lost.jpg", UNPLACED_JPEG]]);
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#track-map.leaflet-container")).toBeVisible();
  expect(await markerPositions(page)).toEqual([]);

  // From the gallery: it is on no marker to be opened from.
  await page.getByTitle("Open lost.jpg").click();
  await page.getByRole("button", { name: "Place on map" }).click();
  const placing = page.getByRole("dialog", { name: "Place lost.jpg" });
  await expect(placing.locator("#place-map.leaflet-container")).toBeVisible();
  await expect(placing.locator("#place-warning")).toHaveCount(0);
  await expect(placing.locator("#place-map .place-current")).toHaveCount(0);

  const map = await placing.locator("#place-map").boundingBox();
  await page.mouse.click(map.x + map.width / 2, map.y + map.height / 2);
  await placing.getByRole("button", { name: "Save" }).click();

  await expect(placing).toHaveCount(0);
  const placed = await storedPhoto(request, id, "lost.jpg");
  expect(placed.location_source).toBe("manual");
  await expect
    .poll(() => markerPositions(page))
    .toEqual([[placed.lat, placed.lon]]);
});
