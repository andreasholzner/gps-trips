// US-97: the Collections screen, driven as the owner drives it (ADR-0012's
// browser layer), and the collections selected on it tagged together
// (US-100).
//
// Scope rule, from the 2026-08-26b amendment: only real user events —
// opening it from the menu, typing in the filter, reloading, clicking a
// row, paging, ticking rows and applying tags. What the table, the paging and the empty states say is asserted in
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

/// The tag US-100's test creates, taken away again afterwards.
const NEW_TAG = `guide-${Math.random().toString(36).slice(2, 8)}`;

test.afterAll(async ({ request }) => {
  for (const id of created.splice(0)) {
    await request.delete(`/api/collections/${id}`);
  }
  const tags = await (await request.get("/api/tags")).json();
  for (const tag of tags.filter((tag) => tag.name === NEW_TAG)) {
    await request.delete(`/api/tags/${tag.id}`);
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
  await expect(page).toHaveURL(new RegExp(`/app/collections/${passes}\\??$`));
});

test("changing the filter goes back to the first page (US-97)", async ({ page, request }) => {
  const prefix = `${TAG}page`;
  for (let n = 0; n < 51; n++) {
    await importCollection(request, `${prefix}-${String(n).padStart(2, "0")}`);
  }
  await page.goto(`/app/collections?q=${prefix}`);
  const pager = page.locator(".pager span");
  await expect(pager).toHaveText("Page 1 of 2");
  await page.getByRole("button", { name: "Next ›" }).click();
  await expect(pager).toHaveText("Page 2 of 2");

  // Typing on: every one still matches, and the list starts over.
  const filter = page.locator("#collection-filter");
  await filter.press("End");
  await filter.pressSequentially("-");

  await expect(page).toHaveURL(new RegExp(`q=${prefix}-`));
  await expect(pager).toHaveText("Page 1 of 2");
  await expect(rows(page).first()).toContainText(`${prefix}-00`);
});

test("Add tags the selected collections at once, a new tag after confirming it (US-100, US-112)", async ({
  page,
  request,
}) => {
  // A word of this test's own: the others list theirs under TAG.
  const word = `${TAG}tagged`;
  const first = await importCollection(request, `${word} Alps`);
  await importCollection(request, `${word} Tyrol`);
  await importCollection(request, `${TAG}apart Lakes`);

  await page.goto(`/app/collections?q=${word}`);
  await expect(rows(page)).toHaveCount(2);
  await page.getByRole("checkbox", { name: "Select all" }).check();
  await expect(page.locator("legend")).toContainText(["Tag selected collections"]);

  await page.getByPlaceholder("add a tag").fill(NEW_TAG);
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText(`Tag "${NEW_TAG}" doesn't exist yet`)).toBeVisible();
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByText(`Added "${NEW_TAG}" to 2 selected.`)).toBeVisible();

  // The selection stays, and the suggestions now offer the tag.
  await expect(page.locator("table tbody input[type=checkbox]:checked")).toHaveCount(2);
  await expect(page.locator(`#bulk-tag-suggestions option[value="${NEW_TAG}"]`)).toHaveCount(1);

  // A tagged collection's page shows it; one not selected does not carry it.
  await page.goto(`/app/collections/${first}`);
  await expect(page.locator(".trip-tags")).toContainText(NEW_TAG);
  const tagged = await (await request.get("/api/tags/overview")).json();
  expect(tagged.find((tag) => tag.name === NEW_TAG).collection_count).toBe(2);
});
