// The SPA driven as a user drives it (ADR-0012's browser layer).
//
// Scope rule, from the amendment: this file covers only what the host-target
// tests cannot reach — real events, and the map/chart that JS interop draws.
// Anything assertable by rendering a component to a string belongs there
// instead, where it runs in milliseconds without a browser.
import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const fixture = (name) =>
  readFileSync(fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url)));

/// Seed the throwaway archive through the real import API (US-1), so the
/// tests read the same data path the app does rather than a hand-built DB.
test.beforeAll(async ({ request }) => {
  const imports = [
    { file: "sample.gpx", fields: { activity_type: "hiking" } },
    { file: "region_alps.gpx", fields: { activity_type: "hiking" } },
    { file: "sample.gpx", fields: { kind: "planned", name: "Planned Ridge Route" } },
  ];
  for (const { file, fields } of imports) {
    const response = await request.post("/api/import", {
      // The import answers 303 to the new trip; following that would report
      // the redirect target's status instead and hide an import failure.
      maxRedirects: 0,
      multipart: {
        gpx: { name: file, mimeType: "application/gpx+xml", buffer: fixture(file) },
        ...fields,
      },
    });
    expect(response.status(), `importing ${file}`).toBe(303);
  }
});

test("the list narrows as the owner types", async ({ page }) => {
  await page.goto("/app/");
  const rows = page.locator("table tbody tr");
  await expect(rows).toHaveCount(2);

  // The event the host-target tests cannot dispatch.
  await page.getByRole("searchbox").fill("inn");

  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("Inn Valley Ride");
});

test("the tabs separate planned trips from recorded ones", async ({ page }) => {
  await page.goto("/app/");
  await page.getByRole("button", { name: "Planned" }).click();

  const rows = page.locator("table tbody tr");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("Planned Ridge Route");
});

test("the detail screen draws the track and the elevation profile", async ({ page }) => {
  await page.goto("/app/");
  await page.getByRole("link", { name: "Oslo Hills Walk" }).click();

  await expect(page.getByRole("heading", { name: "Oslo Hills Walk" })).toBeVisible();
  // Leaflet and uPlot are driven through `document::eval` (ADR-0025); that
  // they actually drew is only observable in a browser.
  await expect(page.locator("#trip-map img.leaflet-tile").first()).toBeVisible();
  await expect(page.locator("#trip-map path").first()).toBeAttached();
  await expect(page.locator("#trip-elevation canvas").first()).toBeVisible();
});

test("a shared link opens the trip directly", async ({ page }) => {
  // A cold load of a client-side route: exercises the server's SPA index
  // fallback, which only exists in the served bundle.
  await page.goto("/app/trips/1");

  await expect(page.getByRole("heading", { name: "Oslo Hills Walk" })).toBeVisible();
  await expect(page.locator("#trip-map img.leaflet-tile").first()).toBeVisible();
});
