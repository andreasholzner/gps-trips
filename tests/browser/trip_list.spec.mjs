// The trip-list screen driven as the owner drives it (US-41, ADR-0012's
// browser layer).
//
// Scope rule, from the amendment: this file covers only what the host-target
// tests cannot reach — real user events. Anything assertable by rendering a
// component to a string belongs in `crates/ui-dioxus`, where it runs in
// milliseconds without a browser. Each test below names the event it needs.
import { expect, signIn, test } from "./session.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const SAMPLE_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/sample.gpx", import.meta.url)),
);

const rows = (page) => page.locator("table tbody tr");

/// Seed the throwaway archive through the real import API (US-1), so these
/// tests read the same data path the app does rather than a hand-built DB.
/// Returns the new trip's id, from the redirect the import answers with.
async function importTrip(request, fields) {
  const response = await request.post("/api/import", {
    // The import answers 303 to the new trip; following that would report the
    // redirect target's status instead and hide an import failure.
    maxRedirects: 0,
    multipart: {
      gpx: {
        name: "track.gpx",
        mimeType: "application/gpx+xml",
        buffer: SAMPLE_GPX,
      },
      ...fields,
    },
  });
  expect(response.status(), `importing ${fields.name}`).toBe(303);
  return Number(response.headers()["location"].replace("/app/trips/", ""));
}

/// A tag no archive can already hold, so the confirm-before-create step
/// (US-33) is genuinely exercised however often this file runs.
const NEW_TAG = `summer-${Math.random().toString(36).slice(2, 8)}`;

// US-19: the archive is gated, so the browser needs a session before any of
// this can run. The seeding `request` fixture arrives with one already
// (`session.mjs`).
test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.beforeAll(async ({ request }) => {
  // Playwright restarts its worker after a failed test, which re-runs this
  // hook. Seeding must therefore be idempotent: otherwise one real failure
  // would duplicate the archive's contents and fail every later test for a
  // reason that has nothing to do with what broke.
  const existing = new Set(
    (await (await request.get("/api/trips")).json()).map((trip) => trip.name),
  );
  const seed = [
    { name: "Oslo Hills Walk", activity_type: "hiking" },
    { name: "Inn Valley Ride", activity_type: "cycling" },
    { name: "Planned Ridge Route", kind: "planned" },
  ];
  const ids = {};
  for (const fields of seed) {
    if (!existing.has(fields.name)) {
      ids[fields.name] = await importTrip(request, fields);
    }
  }

  // One pre-existing tag, so the tag filter has something to offer before
  // any test creates one (US-33).
  const tags = await (await request.get("/api/tags")).json();
  if (!tags.some((tag) => tag.name === "alpine")) {
    const tagged = await request.post(`/api/trips/${ids["Oslo Hills Walk"]}/tags`, {
      data: { name: "alpine" },
    });
    expect(tagged.status(), "tagging the seeded trip").toBe(201);
  }
});

test("the list narrows as the owner types (US-13)", async ({ page }) => {
  // Needs a real `input` event: `dioxus-ssr` renders, it does not type.
  await page.goto("/app/");
  await expect(rows(page)).toHaveCount(2);

  await page.getByRole("searchbox").fill("inn");

  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Inn Valley Ride");
});

// US-52 needs the region rectangle restored "when the list is loaded again",
// which only works if the filters live in the URL. That is a property of the
// address bar and a real reload, so it can only be checked here.
test("a filtered list is in the URL and survives a reload (US-52)", async ({ page }) => {
  await page.goto("/app/");
  await page.getByRole("searchbox").fill("inn");
  await expect(rows(page)).toHaveCount(1);

  // The address bar followed the typing, without stacking a history entry
  // per keystroke: one Back leaves the filtered list behind entirely.
  await expect(page).toHaveURL(/[?&]q=inn/);

  await page.reload();

  await expect(page.getByRole("searchbox")).toHaveValue("inn");
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Inn Valley Ride");
});

test("switching tabs keeps the active filter (US-32)", async ({ page }) => {
  // Needs a real click *and* a preserved input value across it — the one
  // criterion that is only observable once both events have happened.
  await page.goto("/app/");
  await page.getByRole("searchbox").fill("route");

  // Nothing recorded matches, so the filtered-empty state shows rather than
  // the "no trips yet" one.
  await expect(page.getByText("No trips match your filters.")).toBeVisible();

  await page.getByRole("button", { name: "Planned" }).click();

  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Planned Ridge Route");
  await expect(page.getByRole("searchbox")).toHaveValue("route");
});

// US-61: the disclosure is the one thing on this screen whose whole point is
// what happens across two events — it must survive the re-render the second
// one causes. `dioxus-ssr` dispatches neither.
test("the occasional filters open on demand and stay open (US-61)", async ({ page }) => {
  await page.goto("/app/");

  // Closed to begin with, so the table starts near the top of the screen.
  const disclosure = page.locator("details", { hasText: "More filters" }).first();
  await expect(disclosure).not.toHaveAttribute("open", /.*/);
  await expect(page.getByLabel("From")).toBeHidden();

  // The controls the owner reaches for constantly are not behind it.
  await expect(page.getByRole("searchbox")).toBeVisible();
  await expect(page.getByRole("button", { name: "Recorded" })).toBeVisible();

  await page.getByText("More filters").click();
  await expect(page.getByLabel("From")).toBeVisible();

  // Still open after the list re-queries: filtering as you type re-renders
  // this whole screen, and an element rebuilt each keystroke would snap shut.
  await page.getByRole("searchbox").fill("inn");
  await expect(rows(page)).toHaveCount(1);
  await expect(page.getByLabel("From")).toBeVisible();
});

// US-61: the two numbers above the table. They are read off fetches that
// land in either order, which is a race only a real browser runs.
test("the list says how many trips match and how many there are (US-61)", async ({ page }) => {
  await page.goto("/app/");
  await expect(page.getByText("2 recorded trips")).toBeVisible();

  await page.getByRole("searchbox").fill("inn");

  await expect(page.getByText("1 of 2 recorded trips")).toBeVisible();

  // The total follows the tab, and so does the noun.
  await page.getByRole("searchbox").fill("");
  await page.getByRole("button", { name: "Planned" }).click();
  await expect(page.getByText("1 planned trip")).toBeVisible();
});

test("choosing a tag narrows the list to trips carrying it (US-38)", async ({ page }) => {
  // Needs a real `change` event on the checkbox.
  await page.goto("/app/");
  await expect(rows(page)).toHaveCount(2);

  // The tag filter is one of the occasional ones, behind the disclosure
  // (US-61).
  await page.getByText("More filters").click();
  await page.getByRole("checkbox", { name: "alpine" }).check();

  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Oslo Hills Walk");
});

// US-14, carried by US-52. Dragging is a real mouse gesture and the map is
// drawn by JS through `document::eval`, so neither is observable anywhere but
// here — both of this layer's exemptions at once.
test("dragging a rectangle on the map filters by region, and it survives a reload (US-14)", async ({
  page,
}) => {
  await page.goto("/app/");
  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();

  // In view from the start since US-63: nothing to open first.
  await expect(page.locator("#region-map.leaflet-container")).toBeVisible();

  // Arm the drawing, then drag a rectangle over the map's western ocean,
  // nowhere near the fixture's Oslo track.
  //
  // `page.mouse` works in viewport coordinates and does not scroll the way
  // `locator.click()` does, so the map has to be brought into view first or
  // the drag lands on whatever happens to be there instead. Starting a tenth
  // of the way in also keeps clear of Leaflet's zoom control, which swallows
  // mousedown in the top-left corner.
  await page.locator("#region-select").click();
  await page.locator("#region-map").scrollIntoViewIfNeeded();
  const box = await page.locator("#region-map").boundingBox();
  await page.mouse.move(box.x + box.width * 0.1, box.y + box.height * 0.35);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.25, box.y + box.height * 0.75, { steps: 8 });
  await page.mouse.up();

  // The region reached the filters, the query and the URL.
  await expect(page.getByText("No trips match your filters.")).toBeVisible();
  await expect(page).toHaveURL(/[?&]bbox=/);

  // The region outlives a tab switch, like every other filter (US-14).
  await page.getByRole("button", { name: "Planned" }).click();
  await expect(page).toHaveURL(/[?&]bbox=/);
  await expect(page).toHaveURL(/kind=planned/);
  await page.getByRole("button", { name: "Recorded" }).click();

  // And it is restored onto the map on the next load (US-14).
  await page.reload();
  await expect(page.getByText("No trips match your filters.")).toBeVisible();
  await expect(page.locator("#region-map .leaflet-interactive")).toBeVisible();

  // Clearing brings the trips back.
  await page.locator("#region-clear").click();
  await expect(page.getByText("Oslo Hills Walk")).toBeVisible();
  await expect(page).not.toHaveURL(/[?&]bbox=/);
});

// US-58: the same control on the other map. The fix belongs to the control
// rather than to either screen, so it is asserted on both — a rule scoped to
// one screen's container would leave this one broken.
test("the region map's zoom control keeps its own size (US-58)", async ({ page }) => {
  await page.goto("/app/");
  await expect(page.locator("#region-map.leaflet-container")).toBeVisible();

  const bar = await page.locator("#region-map .leaflet-control-zoom").boundingBox();
  for (const label of ["Zoom in", "Zoom out"]) {
    const button = await page.getByRole("button", { name: label }).boundingBox();
    expect(Math.round(button.width), `${label} width`).toBe(30);
    expect(Math.round(button.height), `${label} height`).toBe(30);
    expect(button.y + button.height, `${label} inside the bar`).toBeLessThanOrEqual(
      bar.y + bar.height + 1,
    );
  }
});

// US-63: the marks are drawn by JS through `document::eval`, so only a real
// page shows them — one per matching trip, redrawn as the filters change.
test("the map marks every trip the filters match (US-63)", async ({ page }) => {
  const marks = page.locator("#region-map .heat-mark");
  await page.goto("/app/");

  await expect(marks).toHaveCount(2);

  await page.getByRole("searchbox").fill("inn");
  await expect(rows(page)).toHaveCount(1);
  await expect(marks).toHaveCount(1);
});

// US-63: the view fits the marks once, then stays where the owner puts it
// until they ask. Panning is a real mouse gesture.
test("the map fits the trips on load and again when asked (US-63)", async ({ page }) => {
  const mark = page.locator("#region-map .heat-mark").first();
  const map = page.locator("#region-map");
  await page.goto("/app/");
  await expect(mark).toBeInViewport();

  // Pan the marks off the map: an unarmed drag moves the view.
  await map.scrollIntoViewIfNeeded();
  const box = await map.boundingBox();
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.5);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.9, box.y + box.height * 0.5, { steps: 8 });
  await page.mouse.up();
  const inside = async () => {
    const at = await mark.boundingBox();
    const now = await map.boundingBox();
    return at !== null && at.x >= now.x && at.x + at.width <= now.x + now.width;
  };
  await expect.poll(inside).toBe(false);

  await page.locator("#region-fit").click();
  await expect.poll(inside).toBe(true);
});

test("selected trips are tagged in one go, after confirming a new tag (US-34)", async ({
  page,
}) => {
  // Needs the whole click sequence: select, stage, confirm, apply. The
  // request itself is covered by the api tests in `crates/ui-dioxus`; what
  // is only reachable here is that the screen's controls actually drive it.
  await page.goto("/app/");
  await expect(rows(page)).toHaveCount(2);

  // Select every listed trip at once.
  await page.locator("table thead input[type=checkbox]").check();
  await expect(page.getByRole("button", { name: "Apply to 2 selected" })).toBeVisible();

  // A tag that does not exist yet must be confirmed before it is staged, so
  // a typo cannot quietly become a tag (US-33).
  await page.getByPlaceholder("add a tag").fill(NEW_TAG);
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText(`Tag "${NEW_TAG}" doesn't exist yet`)).toBeVisible();
  await page.getByRole("button", { name: "Create" }).click();

  await page.getByRole("button", { name: "Apply to 2 selected" }).click();

  // The panel goes away with the selection, and the new tag is now a filter
  // choice that lists exactly the trips it was applied to.
  await expect(page.getByRole("button", { name: /Apply to/ })).toBeHidden();
  await page.getByText("More filters").click();
  await page.getByRole("checkbox", { name: NEW_TAG }).check();
  await expect(rows(page)).toHaveCount(2);
});

// US-63's paging. Moving between pages and keeping a selection across them
// are real clicks. Its 51 trips are seeded here and removed again afterwards,
// so every other test in this file keeps its two-trip archive.
test.describe("paging (US-63)", () => {
  const paged = [];

  test.beforeAll(async ({ request }) => {
    for (let n = 0; n < 51; n++) {
      paged.push(await importTrip(request, { name: `Paged Trip ${String(n).padStart(2, "0")}` }));
    }
  });

  test.afterAll(async ({ request }) => {
    for (const id of paged.splice(0)) {
      expect((await request.delete(`/api/trips/${id}`)).status()).toBe(204);
    }
  });

  test("a selection survives paging, and select-all covers one page", async ({ page }) => {
    await page.goto("/app/?q=paged");
    await expect(rows(page)).toHaveCount(50);
    await expect(page.getByText(/^51 of \d+ recorded trips · showing 1–50$/)).toBeVisible();

    await rows(page).first().getByRole("checkbox").check();
    await page.getByRole("button", { name: "Next ›" }).click();

    await expect(rows(page)).toHaveCount(1);
    await expect(page.getByText("Page 2 of 2")).toBeVisible();
    // Select-all here takes this page's one row, not the fifty behind it.
    await page.locator("table thead").getByRole("checkbox").check();
    await expect(page.getByRole("button", { name: "Apply to 2 selected" })).toBeVisible();

    await page.getByRole("button", { name: "‹ Previous" }).click();
    await expect(rows(page).first().getByRole("checkbox")).toBeChecked();
  });

  test("changing the filters goes back to the first page", async ({ page }) => {
    await page.goto("/app/?q=paged");
    await page.getByRole("button", { name: "Next ›" }).click();
    await expect(page.getByText("Page 2 of 2")).toBeVisible();

    await page.getByRole("searchbox").fill("paged trip");

    await expect(rows(page)).toHaveCount(50);
    await expect(page.getByText("Page 1 of 2")).toBeVisible();
    // Where the owner is stays out of the URL (US-52 is about what the list is).
    await expect(page).not.toHaveURL(/page=/);
  });
});
