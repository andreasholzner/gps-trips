// Sharing trips through a link (US-53) and stopping one (US-69), driven as
// the owner and the recipient drive it.
//
// Scope rule, from ADR-0012's 2026-08-26b amendment: both exemptions apply.
// **Real user events** — the owner clicking "Share" and "Create", or "Stop
// sharing" and its confirmation, the recipient clicking a line on the map. **JS-interop rendering** — the app
// tells a share's link from the owner's archive by reading the page's path
// through `document::eval`, and the overview map is Leaflet's. What the
// screens render is asserted in `crates/ui-dioxus/src/shared/tests.rs`,
// `share.rs` and `shares.rs`, and what a share reaches in
// `tests/it/us53_share.rs` and `us69_shares.rs`.
import { expect, signIn, test } from "./session.mjs";
import { ownTrips } from "./trips.mjs";

const trip = ownTrips(test);

/// A browser with no session at all — the recipient's.
async function recipientPage(browser, baseURL) {
  const context = await browser.newContext({ baseURL });
  return { context, page: await context.newPage() };
}

test("the owner shares a trip from its page, and the link opens it without a password (US-53)", async ({
  page,
  request,
  browser,
  baseURL,
}) => {
  const id = await trip(request, "Shared Walk");
  const name = (await (await request.get(`/api/trips/${id}`)).json()).name;
  await signIn(page.request);

  await page.goto(`/app/trips/${id}`);
  await page.locator("#share-trip").click();
  await page.locator('input[name="share-label"]').fill("For Kari");
  await page.locator("#create-share").click();
  const link = await page.locator("#share-link").inputValue();
  expect(link).toMatch(/\/app\/s\/[0-9a-f]{64}$/);

  const recipient = await recipientPage(browser, baseURL);
  await recipient.page.goto(link);
  await expect(recipient.page.getByRole("heading", { name })).toBeVisible();
  await expect(recipient.page.locator(".share-title")).toHaveText("For Kari");
  await expect(recipient.page.locator("#track-map.leaflet-container")).toBeVisible();
  // No login screen, no menu, nothing that changes the trip.
  await expect(recipient.page.locator("#login-password")).toHaveCount(0);
  await expect(recipient.page.locator("#share-trip")).toHaveCount(0);
  await expect(recipient.page.locator("#delete-trip")).toHaveCount(0);
  await recipient.context.close();
});

test("a share of several opens on a map of every track, and a line opens its trip (US-53)", async ({
  request,
  browser,
  baseURL,
}) => {
  const first = await trip(request, "Day one");
  const second = await trip(request, "Day two");
  const created = await request.post("/api/shares", {
    data: { trip_ids: [first, second], label: "Lofoten" },
  });
  expect(created.status(), "creating the share").toBe(201);
  const { token } = await created.json();

  const recipient = await recipientPage(browser, baseURL);
  await recipient.page.goto(`/app/s/${token}`);
  await expect(recipient.page.getByRole("heading", { name: "Lofoten" })).toBeVisible();
  const lines = recipient.page.locator("#overview-map path.leaflet-interactive");
  await expect(lines).toHaveCount(2);

  // Both fixtures are the same track, so either line is a trip of the share.
  // Dispatched on the line itself: a pointer aimed at the middle of a track's
  // bounding box misses a track that curves.
  await lines.first().dispatchEvent("click");
  await expect(recipient.page).toHaveURL(new RegExp(`/app/s/${token}/trips/(${first}|${second})$`));
  await expect(recipient.page.locator("#track-map.leaflet-container")).toBeVisible();

  // The title leads back to the share's list.
  await recipient.page.locator(".share-title a").click();
  await expect(recipient.page.locator("#shared-trips tbody tr")).toHaveCount(2);
  await recipient.context.close();
});

test("a link that opens nothing says so (US-53)", async ({ browser, baseURL }) => {
  const recipient = await recipientPage(browser, baseURL);
  await recipient.page.goto(`/app/s/${"0".repeat(64)}`);
  await expect(recipient.page.locator(".error")).toContainText("This link does not work");
  await expect(recipient.page.locator("#login-password")).toHaveCount(0);
  await recipient.context.close();
});

test("the owner stops a share from the shares screen, and its link then opens nothing (US-69)", async ({
  page,
  request,
  browser,
  baseURL,
}) => {
  const id = await trip(request, "Stopped Walk");
  const created = await request.post("/api/shares", {
    data: { trip_ids: [id], label: "For Ola" },
  });
  expect(created.status(), "creating the share").toBe(201);
  const { token } = await created.json();
  const listed = await (await request.get("/api/shares")).json();
  const shareId = listed.find((share) => share.token === token).id;
  await signIn(page.request);

  await page.goto("/app/shares");
  const row = page.locator(`#share-${shareId}`);
  await expect(row.getByRole("heading", { name: "For Ola" })).toBeVisible();
  await row.getByRole("button", { name: "Stop sharing" }).click();
  await row.getByRole("button", { name: "Stop it" }).click();
  await expect(row).toHaveCount(0);

  const recipient = await recipientPage(browser, baseURL);
  await recipient.page.goto(`/app/s/${token}`);
  await expect(recipient.page.locator(".error")).toContainText("This link does not work");
  await recipient.context.close();
});
