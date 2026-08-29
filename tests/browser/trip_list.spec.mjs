// The trip-list screen driven as the owner drives it (US-41, ADR-0012's
// browser layer).
//
// Scope rule, from the amendment: this file covers only what the host-target
// tests cannot reach — real user events. Anything assertable by rendering a
// component to a string belongs in `crates/ui-dioxus`, where it runs in
// milliseconds without a browser. Each test below names the event it needs.
import { test, expect } from "@playwright/test";
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
  return Number(response.headers()["location"].replace("/trips/", ""));
}

/// A tag no archive can already hold, so the confirm-before-create step
/// (US-33) is genuinely exercised however often this file runs.
const NEW_TAG = `summer-${Math.random().toString(36).slice(2, 8)}`;

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

test("choosing a tag narrows the list to trips carrying it (US-38)", async ({ page }) => {
  // Needs a real `change` event on the checkbox.
  await page.goto("/app/");
  await expect(rows(page)).toHaveCount(2);

  await page.getByRole("checkbox", { name: "alpine" }).check();

  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Oslo Hills Walk");
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
  await page.getByRole("checkbox", { name: NEW_TAG }).check();
  await expect(rows(page)).toHaveCount(2);
});
