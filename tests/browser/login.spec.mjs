// Signing in and out, driven as the owner drives it (US-19, ADR-0012's
// browser layer).
//
// Scope rule, from the 2026-08-26b amendment: this file covers only what the
// host-target tests structurally cannot — **real user events**. Typing a
// password into a field and submitting the form is the whole subject here,
// and `dioxus-ssr` dispatches neither. What the login screen *renders* (the
// field, the archive's own words for a refusal) is asserted in
// `crates/ui-dioxus/src/login.rs`, where it runs without a browser.
//
// It is also the only place the cookie itself can be exercised end to end:
// the archive sets it, the browser keeps it, and the SPA's own fetches carry
// it without the page ever seeing it.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, PASSWORD, signIn, test } from "./session.mjs";

const SAMPLE_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/sample.gpx", import.meta.url)),
);

const passwordField = (page) => page.locator("#login-password");
const signInButton = (page) => page.locator("#login-submit");

test("the archive asks for the password before it shows anything (US-19)", async ({ page }) => {
  await page.goto("/app/");

  await expect(passwordField(page)).toBeVisible();
  // Its own screen, not the browser's credential dialog — which is what
  // ADR-0010's amendment gave up basic auth for.
  await expect(page.locator("table")).toHaveCount(0);
});

test("the right password opens the archive (US-19)", async ({ page }) => {
  await page.goto("/app/");

  await passwordField(page).fill(PASSWORD);
  await signInButton(page).click();

  await expect(page.getByRole("heading", { name: "Trips" })).toBeVisible();
  await expect(passwordField(page)).toHaveCount(0);
});

test("a wrong password says so, in the archive's own words (US-19)", async ({ page }) => {
  await page.goto("/app/");

  await passwordField(page).fill("not the password");
  await signInButton(page).click();

  await expect(page.locator("#login-error")).toBeVisible();
  // Refused, and still refusing: nothing of the archive is behind it.
  await expect(page.getByRole("heading", { name: "Trips" })).toHaveCount(0);
});

test("signing in lands on the screen that was asked for (US-19)", async ({ page, request }) => {
  // The criterion this test exists for: a bookmark opened cold shows the
  // login screen and then *that* screen, not the trip list. It works because
  // signing in never navigates — the URL was right all along — which is only
  // observable with a real address bar and a real submit.
  const imported = await request.post("/api/import", {
    maxRedirects: 0,
    multipart: {
      gpx: { name: "track.gpx", mimeType: "application/gpx+xml", buffer: SAMPLE_GPX },
      name: "Bookmarked Walk",
    },
  });
  expect(imported.status(), "seeding the bookmarked trip").toBe(303);
  const id = Number(imported.headers()["location"].replace("/app/trips/", ""));

  await page.goto(`/app/trips/${id}`);
  await passwordField(page).fill(PASSWORD);
  await signInButton(page).click();

  await expect(page.getByRole("heading", { name: "Bookmarked Walk" })).toBeVisible();
  await expect(page).toHaveURL(new RegExp(`/app/trips/${id}$`));

  await request.delete(`/api/trips/${id}`);
});

test("an old bookmark to a retired page reaches the login screen (US-19)", async ({ page }) => {
  // The redirects left behind by US-42/43/44 are allowlisted precisely so
  // this lands on a screen rather than on a JSON refusal in the address bar.
  await page.goto("/import");

  await expect(page).toHaveURL(/\/app\/import$/);
  await expect(passwordField(page)).toBeVisible();
});

test("signing out ends the session (US-19)", async ({ page }) => {
  await signIn(page.request);
  await page.goto("/app/");
  await expect(page.getByRole("heading", { name: "Trips" })).toBeVisible();

  await page.locator("#sign-out").click();

  await expect(passwordField(page)).toBeVisible();
  // And it is gone from the browser too, not merely from the screen: a
  // reload finds no session to resume.
  await page.reload();
  await expect(passwordField(page)).toBeVisible();
});

test("a session survives a reload (US-19)", async ({ page }) => {
  await page.goto("/app/");
  await passwordField(page).fill(PASSWORD);
  await signInButton(page).click();
  await expect(page.getByRole("heading", { name: "Trips" })).toBeVisible();

  await page.reload();

  await expect(page.getByRole("heading", { name: "Trips" })).toBeVisible();
  await expect(passwordField(page)).toHaveCount(0);
});
