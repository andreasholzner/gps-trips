// US-97: the Collections screen, driven as the owner drives it (ADR-0012's
// browser layer).
//
// Scope rule, from the 2026-08-26b amendment: only real user events —
// opening it from the menu, typing in the filter, reloading, clicking a
// row. What the table, the paging and the empty states say is asserted in
// `crates/ui-dioxus` (`collections`), without a browser.
import { expect, signIn, test } from "./session.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/// Two waypoints and no track: the smallest collection there is.
const WAYPOINTS_ONLY_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/waypoints_only.gpx", import.meta.url)),
);

/// A word no other spec's names hold: the suite shares one archive.
const TAG = `shelf${Math.random().toString(36).slice(2, 8)}`;

/// Every collection these tests create, taken away again afterwards.
const created = [];

const rows = (page) => page.locator("table tbody tr");

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  for (const id of created.splice(0)) {
    await request.delete(`/api/collections/${id}`);
  }
});

async function importCollection(request, name) {
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: { name: "book.gpx", mimeType: "application/gpx+xml", buffer: WAYPOINTS_ONLY_GPX },
      name,
    },
  });
  expect(response.status(), `importing ${name}`).toBe(201);
  const { id } = await response.json();
  created.push(id);
  return id;
}

test("the menu opens the collections, and the filter narrows them, stays in the URL and survives a reload (US-97)", async ({
  page,
  request,
}) => {
  await importCollection(request, `${TAG} Lakes`);
  const passes = await importCollection(request, `${TAG} Passes`);
  await page.goto("/app/");
  const menu = page.locator("#app-menu a");
  await expect(menu.nth(3)).toHaveText("Tags");
  await expect(menu.nth(4)).toHaveText("Collections");
  await menu.nth(4).click();
  await expect(page).toHaveURL(/\/app\/collections\??$/);

  await page.locator("#collection-filter").fill(TAG.toUpperCase());
  await expect(rows(page)).toHaveCount(2);
  await page.locator("#collection-filter").pressSequentially(" PA");

  await expect(rows(page)).toHaveCount(1);
  await expect(page).toHaveURL(/\/app\/collections\?q=/);
  await page.reload();
  await expect(page.locator("#collection-filter")).toHaveValue(`${TAG.toUpperCase()} PA`);
  await expect(rows(page)).toHaveCount(1);

  await rows(page)
    .getByRole("link", { name: `${TAG} Passes` })
    .click();
  await expect(page).toHaveURL(new RegExp(`/app/collections/${passes}$`));
});
