// The trip-list map zoomed in (US-73): past a zoom threshold it draws the
// trips in view as their tracks rather than heat marks.
//
// Zooming, hovering, clicking a line and dragging from one are real user
// events on a map drawn by JS through `document::eval` — this layer's
// exemptions (ADR-0012). Which trips get a line, and in which color, is
// asserted in `crates/ui-dioxus` (`trip_lines`).
import { expect, signIn, test } from "./session.mjs";
import { zoomBy } from "./map.mjs";
import { ownTrips, UNTIMED_GPX } from "./trips.mjs";

const ownTrip = ownTrips(test);

const lines = (page) => page.locator("#region-map .trip-line");
const marks = (page) => page.locator("#region-map .heat-mark");

/// The list narrowed by name to `q`, so only this test's trips are on the
/// map: every fixture track is the same walk in Oslo.
const listOf = (q) => `/app/?q=${encodeURIComponent(q)}`;

/// The trip's name, as the import gave it.
const nameOf = async (request, id) => (await (await request.get(`/api/trips/${id}`)).json()).name;

/// A point on the line itself, halfway along it, in viewport coordinates.
const onTheLine = (line) =>
  line.evaluate((path) => {
    const at = path
      .getPointAtLength(path.getTotalLength() / 2)
      .matrixTransform(path.getScreenCTM());
    return { x: at.x, y: at.y };
  });

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test("zoomed in, the trips are lines; zoomed out, marks again", async ({ page, request }) => {
  const id = await ownTrip(request, "Lines Walk", "hiking");
  const name = await nameOf(request, id);
  await page.goto(listOf(name));

  // Fitted to one short walk, the map is zoomed in past the threshold.
  await expect(lines(page)).toHaveCount(1);
  await expect(marks(page)).toHaveCount(0);
  await expect(lines(page)).toHaveAttribute("stroke", "#b2182b");

  // Fitted to a single short walk, it is at 12 — the "Fit to trips" cap.
  // The threshold is 7, so six steps out is the first zoom with marks.
  await zoomBy(page, "region-map", "Zoom out", 6, 12);
  await expect(marks(page)).toHaveCount(1);
  await expect(lines(page)).toHaveCount(0);

  await zoomBy(page, "region-map", "Zoom in", 1, 6);
  await expect(lines(page)).toHaveCount(1);
  await expect(marks(page)).toHaveCount(0);
});

test("a track already read is not fetched again", async ({ page, request }) => {
  const id = await ownTrip(request, "Cached Walk", "hiking");
  const asked = [];
  page.on("request", (sent) => {
    if (sent.url().includes("/api/trips/tracks")) asked.push(sent.url());
  });

  await page.goto(listOf(await nameOf(request, id)));
  await expect(lines(page)).toHaveCount(1);
  expect(asked).toHaveLength(1);

  // Out and back in, still past the threshold: the view settles twice, and
  // the line is redrawn from what the map already holds. The drawn line is
  // marked, so its redrawn successor shows the redraw has happened.
  await lines(page).evaluate((path) => path.setAttribute("data-before", ""));
  await zoomBy(page, "region-map", "Zoom out", 1, 12);
  await zoomBy(page, "region-map", "Zoom in", 1, 11);
  await expect(page.locator("#region-map .trip-line[data-before]")).toHaveCount(0);
  await expect(lines(page)).toHaveCount(1);
  expect(asked).toHaveLength(1);
});

test("zooming in while the tracks load does not ask for them again", async ({ page, request }) => {
  const id = await ownTrip(request, "Zoomed Walk", "hiking");
  const asked = [];
  let release;
  const held = new Promise((resolve) => (release = resolve));
  await page.route("**/api/trips/tracks?*", async (route) => {
    asked.push(route.request().url());
    await held;
    await route.continue();
  });

  await page.goto(listOf(await nameOf(request, id)));
  await expect.poll(() => asked.length).toBe(1);
  // A narrower view settles while the first answer is still on its way.
  await zoomBy(page, "region-map", "Zoom in", 1, 12);
  release();

  await expect(lines(page)).toHaveCount(1);
  await expect(page.locator("#region-map-busy")).toHaveCount(0);
  expect(asked).toHaveLength(1);
});

test("the map says it is busy while the tracks load", async ({ page, request }) => {
  const id = await ownTrip(request, "Awaited Walk", "hiking");
  // Hold the tracks back until the busy sign has been seen.
  let release;
  const held = new Promise((resolve) => (release = resolve));
  await page.route("**/api/trips/tracks?*", async (route) => {
    await held;
    await route.continue();
  });
  const busy = page.locator("#region-map-busy");

  await page.goto(listOf(await nameOf(request, id)));
  await expect(busy).toHaveText("Loading tracks…");
  // It only shows once the wait is long enough to notice, so a quick load
  // does not blink it: held back at first, then faded in.
  const shown = () => busy.evaluate((sign) => getComputedStyle(sign).opacity);
  expect(await busy.evaluate((sign) => getComputedStyle(sign).animationDelay)).toBe("0.3s");
  await expect.poll(shown).toBe("1");

  release();
  await expect(lines(page)).toHaveCount(1);
  await expect(busy).toHaveCount(0);
});

test("a line is named and highlighted on hover, and a click opens its trip", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Hovered Walk", "hiking");
  const name = await nameOf(request, id);
  await page.goto(listOf(name));
  await expect(lines(page)).toHaveCount(1);
  await page.locator("#region-map").scrollIntoViewIfNeeded();

  await expect(lines(page)).toHaveAttribute("stroke-width", "3");
  const at = await onTheLine(lines(page));
  await page.mouse.move(at.x, at.y);
  await expect(page.locator("#region-map .leaflet-tooltip")).toHaveText(name);
  await expect(lines(page)).toHaveAttribute("stroke-width", "6");

  // Off the line, it is drawn as before.
  await page.mouse.move(at.x, at.y - 60);
  await expect(lines(page)).toHaveAttribute("stroke-width", "3");
  await page.mouse.move(at.x, at.y);

  await page.mouse.click(at.x, at.y);
  await expect(page).toHaveURL(new RegExp(`/app/trips/${id}$`));
});

test("a change of filters redraws the lines, in shades per trip", async ({ page, request }) => {
  const tag = `shade${Math.random().toString(36).slice(2, 8)}`;
  const first = await nameOf(request, await ownTrip(request, `${tag} One`, "hiking"));
  await ownTrip(request, `${tag} Two`, "hiking");
  await page.goto(listOf(tag));

  // Two hikes: the activity's color and its next shade (US-72).
  await expect(lines(page)).toHaveCount(2);
  const strokes = await lines(page).evaluateAll((paths) =>
    paths.map((path) => path.getAttribute("stroke")).sort(),
  );
  expect(strokes).toEqual(["#9b2543", "#b2182b"]);

  await page.getByRole("searchbox").fill(first);
  await expect(lines(page)).toHaveCount(1);
});

test("a read of the tracks that fails is not asked again until the view moves", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Failing Walk", "hiking");
  const asked = [];
  await page.route("**/api/trips/tracks?*", async (route) => {
    asked.push(route.request().url());
    await route.fulfill({ status: 500, contentType: "application/json", body: "{}" });
  });

  await page.goto(listOf(await nameOf(request, id)));
  await expect.poll(() => asked.length).toBe(1);
  await expect(page.locator("#region-map-busy")).toHaveCount(0);
  // Nothing to wait *for* when nothing should happen: give a retry the time
  // it would take, then count.
  await page.waitForTimeout(500);
  expect(asked).toHaveLength(1);

  // A view that moves asks once more.
  await zoomBy(page, "region-map", "Zoom out", 1, 12);
  await expect.poll(() => asked.length).toBe(2);
});

test("a kind switched off takes its lines off while the new ones load (US-90)", async ({
  page,
  request,
}) => {
  const tag = `switch${Math.random().toString(36).slice(2, 8)}`;
  await ownTrip(request, `${tag} Recorded`, "hiking");
  // The same walk, planned: in view wherever the recorded one is.
  const planned = await request.post("/api/import", {
    maxRedirects: 0,
    multipart: {
      gpx: { name: "track.gpx", mimeType: "application/gpx+xml", buffer: UNTIMED_GPX },
      name: `${tag} Planned`,
      kind: "planned",
    },
  });
  expect(planned.status()).toBe(303);
  const plannedId = Number(planned.headers()["location"].replace("/app/trips/", ""));
  try {
    await page.goto(listOf(tag));
    await expect(lines(page)).toHaveCount(1);
    let release;
    const held = new Promise((resolve) => (release = resolve));
    await page.route("**/api/trips/tracks?*", async (route) => {
      await held;
      await route.continue();
    });

    // Planned on, recorded off: the list no longer holds the recorded walk.
    await page.getByRole("button", { name: "Planned" }).click();
    await page.getByRole("button", { name: "Recorded" }).click();

    // The recorded walk's line goes; the planned one is a mark until its
    // track arrives.
    await expect(lines(page)).toHaveCount(0);
    await expect(marks(page)).toHaveCount(1);
    release();
    await expect(lines(page)).toHaveCount(1);
  } finally {
    await request.delete(`/api/trips/${plannedId}`);
  }
});

test("recorded and planned trips are drawn together, each in its line style (US-90)", async ({
  page,
  request,
}) => {
  const tag = `styles${Math.random().toString(36).slice(2, 8)}`;
  await ownTrip(request, `${tag} Recorded`, "hiking");
  const planned = await request.post("/api/import", {
    maxRedirects: 0,
    multipart: {
      gpx: { name: "track.gpx", mimeType: "application/gpx+xml", buffer: UNTIMED_GPX },
      name: `${tag} Planned`,
      kind: "planned",
    },
  });
  expect(planned.status()).toBe(303);
  const plannedId = Number(planned.headers()["location"].replace("/app/trips/", ""));
  try {
    await page.goto(`${listOf(tag)}&kind=recorded%2Cplanned`);
    await expect(lines(page)).toHaveCount(2);

    // Solid for the recorded walk, dashed for the planned one.
    const dashes = await lines(page).evaluateAll((paths) =>
      paths.map((path) => path.getAttribute("stroke-dasharray")).sort(),
    );
    expect(dashes).toEqual(["8 6", null]);

    // A click on the planned line opens the planned trip.
    const dashed = page.locator('#region-map .trip-line[stroke-dasharray="8 6"]');
    await page.locator("#region-map").scrollIntoViewIfNeeded();
    const at = await onTheLine(dashed);
    await page.mouse.click(at.x, at.y);
    await expect(page).toHaveURL(new RegExp(`/app/trips/${plannedId}$`));
  } finally {
    await request.delete(`/api/trips/${plannedId}`);
  }
});
