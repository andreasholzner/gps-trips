// US-83: the Tags screen, driven as the owner drives it (ADR-0012's browser
// layer).
//
// Scope rule, from the 2026-08-26b amendment: only real user events —
// typing in the filter, deleting through the confirmation, creating a tag.
// What the rows, the paging and the confirmation say is asserted in
// `crates/ui-dioxus` (`tags`), without a browser.
import { expect, signIn, test } from "./session.mjs";
import { ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

/// A tag name of this run's own: the suite shares one archive.
const fresh = (label) => `${label}-${Math.random().toString(36).slice(2, 8)}`;

/// Every tag a test here made, deleted afterwards so the other specs'
/// tag choices stay as they seeded them.
const created = [];

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  const tags = await (await request.get("/api/tags")).json();
  for (const tag of tags.filter((tag) => created.includes(tag.name))) {
    await request.delete(`/api/tags/${tag.id}`);
  }
});

async function createTag(request, name) {
  created.push(name);
  const response = await request.post("/api/tags", { data: { name } });
  expect(response.status(), `creating ${name}`).toBe(201);
  return (await response.json()).id;
}

const rows = (page) => page.locator("table.tags tbody tr[id^=tag-]");

test("the menu opens the tags, and the filter narrows them, stays in the URL and survives a reload (US-83)", async ({
  page,
  request,
}) => {
  const prefix = fresh("filter");
  for (const name of [`${prefix}-alps`, `${prefix}-norway`, `${prefix}-oslo`]) {
    await createTag(request, name);
  }
  await page.goto("/app/");
  const menu = page.locator("#app-menu a");
  await expect(menu.nth(2)).toHaveText("Statistics");
  await expect(menu.nth(3)).toHaveText("Tags");
  await menu.nth(3).click();
  await expect(page).toHaveURL(/\/app\/tags/);

  await page.locator("#tag-filter").fill(prefix.toUpperCase());
  await expect(rows(page)).toHaveCount(3);
  await page.locator("#tag-filter").pressSequentially("-O");

  await expect(rows(page)).toHaveCount(1);
  await expect(page).toHaveURL(new RegExp(`q=${prefix.toUpperCase()}-O`));
  await page.reload();
  await expect(page.locator("#tag-filter")).toHaveValue(`${prefix.toUpperCase()}-O`);
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText(`${prefix}-oslo`);
});

test("deleting a tag is confirmed first, says what becomes of its share, and takes it off its trips (US-83)", async ({
  page,
  request,
}) => {
  const name = fresh("doomed");
  const trip = await ownTrip(request, "Tagged Walk", "hiking");
  created.push(name);
  const tagged = await request.post(`/api/trips/${trip}/tags`, { data: { name } });
  expect(tagged.status()).toBe(201);
  const shared = await request.post("/api/shares", { data: { tags: [name], label: "Doomed" } });
  expect(shared.status()).toBe(201);
  const { token } = await shared.json();

  await page.goto(`/app/tags?q=${name}`);
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText(`${name} (1)`);
  await rows(page).first().getByRole("button", { name: "Delete" }).click();

  const confirmation = page.locator(".confirm");
  await expect(confirmation).toContainText(`Delete the tag “${name}”?`);
  await expect(confirmation).toContainText("The share “Doomed” is stopped.");
  await confirmation.getByRole("button", { name: "Delete it" }).click();

  await expect(page.getByText(`No tag contains “${name}”.`)).toBeVisible();
  const onTrip = await (await request.get(`/api/trips/${trip}/tags`)).json();
  expect(onTrip).toEqual([]);
  expect((await request.get(`/api/trips/${trip}`)).status()).toBe(200);
  expect((await page.request.get(`/s/${token}/api/share`)).status()).toBe(404);
});

test("a created tag shows at once, and a refused name says why (US-83)", async ({ page }) => {
  const name = fresh("new");
  created.push(name);
  await page.goto(`/app/tags?q=${name}`);
  await expect(page.getByText(`No tag contains “${name}”.`)).toBeVisible();

  await page.locator("#new-tag-name").fill(name.toUpperCase());
  await page.getByRole("button", { name: "Create tag" }).click();

  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText(`${name} (0)`);
  await expect(page.locator("#new-tag-name")).toHaveValue("");

  await page.locator("#new-tag-name").fill(name);
  await page.getByRole("button", { name: "Create tag" }).click();
  await expect(page.locator("#create-tag-error")).toHaveText(
    `Could not create the tag: tag "${name}" already exists`,
  );
  await expect(rows(page)).toHaveCount(1);

  await page.locator("#new-tag-name").fill("day trip");
  await page.getByRole("button", { name: "Create tag" }).click();
  await expect(page.locator("#create-tag-error")).toHaveText(
    "Could not create the tag: tag name cannot contain spaces",
  );
});
