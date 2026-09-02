// Signing in, for the browser layer (US-19).
//
// The archive refuses to start without a shared password, so every spec here
// runs against a gated server and has to hold a session — the same position
// the owner's browser is in. Two clients need one, and they hold it
// differently, which is the same split the mechanism itself has:
//
//   * **the browser** keeps the cookie the archive sets. `signIn(page.request)`
//     posts the login through the page's own context, so the cookie lands
//     where the SPA's fetches will find it, and Chromium attaches it from
//     then on.
//   * **the `request` fixture**, which seeds trips through the real import
//     API, carries a `Bearer` token instead — overridden below. Playwright's
//     API client stores a `Secure` cookie but will not send it over plain
//     `http://`, loopback included; a real browser makes the loopback
//     exception, and the deployed instance is HTTPS-only anyway (ADR-0023).
//     So this client is in exactly the position the token exists for: one
//     with no usable cookie store, like the Android app's (US-16).
//
// Signing in without the form is a shortcut on purpose: the form itself is
// what `login.spec.mjs` drives, and repeating it in every other spec's setup
// would buy nothing and cost a page load each time.
import { test as base, expect, request as apiRequest } from "@playwright/test";

/// The password `playwright.config.mjs` starts the server with.
export const PASSWORD = process.env.TRIP_ARCHIVE_PASSWORD ?? "browser-test-password";

/// Open a session in a context that keeps cookies — a page's own
/// `page.request`, whose jar is the browser's.
export async function signIn(context) {
  const response = await context.post("/api/session", { data: { password: PASSWORD } });
  expect(response.status(), "signing in").toBe(200);
  return (await response.json()).token;
}

/// A session token, fetched with a throwaway anonymous client.
async function token(baseURL) {
  const anonymous = await apiRequest.newContext({ baseURL });
  const value = await signIn(anonymous);
  await anonymous.dispose();
  return value;
}

/// `test`, with the `request` fixture signed in. Specs import it from here
/// instead of from `@playwright/test`, and their seeding calls are unchanged.
export const test = base.extend({
  request: async ({ playwright, baseURL }, use) => {
    const context = await playwright.request.newContext({
      baseURL,
      extraHTTPHeaders: { Authorization: `Bearer ${await token(baseURL)}` },
    });
    await use(context);
    await context.dispose();
  },
});

export { expect };
