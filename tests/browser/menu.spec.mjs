// The app's navigation menu (US-60), driven as the owner drives it.
//
// Scope rule, from ADR-0012's 2026-08-26b amendment: this file covers only
// what the host-target tests cannot reach. What the menu *renders* is
// asserted in `crates/ui-dioxus/src/menu.rs`, without a browser. What needs
// one is here: opening and closing are real clicks and key presses, and
// which of the two shapes the menu takes is a media query, which exists only
// once a browser has laid the page out at a given width.
import { expect, signIn, test } from "./session.mjs";

const items = ["All trips", "Import a trip", "Sync with Komoot", "Sign out"];
const burger = (page) => page.locator("#app-menu-button");
const item = (page, name) => page.locator("#app-menu").getByText(name, { exact: true });

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

// Every screen renders inside the menu, which is the point of it being a
// layout route: the import and sync screens had no way back to the list
// except their own link, and that link is gone.
test("every screen carries the menu, with everything that left the pages (US-60)", async ({
  page,
  request,
}) => {
  const imported = await request.post("/api/import", {
    maxRedirects: 0,
    multipart: {
      gpx: {
        name: "track.gpx",
        mimeType: "application/gpx+xml",
        buffer: Buffer.from(
          '<?xml version="1.0"?><gpx version="1.1" creator="t"><trk><name>Menu Trip</name><trkseg>' +
            '<trkpt lat="59.91" lon="10.75"><ele>10</ele><time>2024-06-01T08:00:00Z</time></trkpt>' +
            '<trkpt lat="59.92" lon="10.76"><ele>20</ele><time>2024-06-01T08:30:00Z</time></trkpt>' +
            "</trkseg></trk></gpx>",
        ),
      },
      name: `Menu Trip ${Math.random().toString(36).slice(2, 8)}`,
    },
  });
  expect(imported.status(), "seeding a trip").toBe(303);
  const id = Number(imported.headers()["location"].replace("/app/trips/", ""));

  for (const path of ["/app/", `/app/trips/${id}`, "/app/import", "/app/komoot/sync"]) {
    await page.goto(path);
    for (const name of items) {
      await expect(item(page, name), `${name} on ${path}`).toBeVisible();
    }
    // The links each screen used to carry are gone, not duplicated.
    await expect(page.getByText("← All trips")).toHaveCount(0);
  }

  await request.delete(`/api/trips/${id}`);
});

// A desktop viewport (the suite's default, 1280px wide): the items are the
// navigation, and there is no burger to press.
test("on a wide screen the items are in the header, with no burger (US-60)", async ({ page }) => {
  await page.goto("/app/");

  await expect(burger(page)).toBeHidden();
  for (const name of items) {
    await expect(item(page, name)).toBeVisible();
  }

  // Sign out is the rarest thing in the menu and must not be the loudest:
  // Pico makes every button a filled block, which put a solid slab beside
  // three quiet links.
  const signOut = await page.locator("#sign-out").evaluate((el) => {
    const style = getComputedStyle(el);
    return { background: style.backgroundColor, border: style.borderStyle };
  });
  expect(signOut.background, "not a filled block").toBe("rgba(0, 0, 0, 0)");
  expect(signOut.border, "nor an outlined one").toBe("none");

  // Quiet, but on the same line as the links it sits beside. Styling it down
  // to their weight is only half the job: it is still a button among anchors,
  // and a row of items of unequal height only reads as one row if their text
  // sits on one baseline. Measured rather than eyeballed, because a few
  // pixels of drift is exactly the amount that looks like a mistake without
  // announcing what it is.
  const baselines = await page
    .locator("#app-menu")
    .evaluate((menu) =>
      [...menu.children].map((el) => {
        const range = document.createRange();
        range.selectNodeContents(el);
        // The bottom of the text itself, not of the box around it.
        return Math.round(range.getBoundingClientRect().bottom);
      }),
    );
  expect(new Set(baselines).size, `menu items sit on ${baselines}`).toBe(1);

  // And they navigate without a page load, like every other link in the SPA.
  await page.evaluate(() => {
    window.__stillTheSameDocument = true;
  });
  await item(page, "Import a trip").click();
  await expect(page.getByRole("heading", { name: "Import a trip" })).toBeVisible();
  expect(await page.evaluate(() => window.__stillTheSameDocument)).toBe(true);
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("the items are behind the burger, which closes again (US-60)", async ({ page }) => {
    await page.goto("/app/");

    // Closed to begin with: the burger is the only way in, and it says so.
    await expect(burger(page)).toBeVisible();
    await expect(burger(page)).toHaveAttribute("aria-expanded", "false");
    await expect(item(page, "Import a trip")).toBeHidden();

    // And it can actually be seen: Pico sets `--pico-color` on a button to
    // the inverse it uses for text on a filled background, so a glyph that
    // takes its colour from there is white on white — visible to a test that
    // only measures a box, and invisible to the owner.
    const glyph = await burger(page).evaluate((el) => ({
      ink: getComputedStyle(el).color,
      paper: getComputedStyle(document.body).backgroundColor,
    }));
    expect(glyph.ink, "the burger is not painted in the page's own colour").not.toBe(glyph.paper);

    await burger(page).click();
    await expect(burger(page)).toHaveAttribute("aria-expanded", "true");
    for (const name of items) {
      await expect(item(page, name)).toBeVisible();
    }

    // A click anywhere else closes it — the backdrop covers the page, so even
    // a map or a chart, which swallow their own pointer events, count as
    // "elsewhere".
    await page.locator(".menu-backdrop").click({ position: { x: 10, y: 400 } });
    await expect(item(page, "Import a trip")).toBeHidden();

    // So does Escape, for a menu opened from the keyboard.
    await burger(page).click();
    await expect(item(page, "Import a trip")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(item(page, "Import a trip")).toBeHidden();

    // And choosing an item both navigates and closes it: a panel left open
    // over the screen it just went to would cover the answer.
    await burger(page).click();
    await item(page, "Sync with Komoot").click();
    await expect(page.getByRole("heading", { name: "Sync with Komoot" })).toBeVisible();
    await expect(item(page, "Sync with Komoot")).toBeHidden();
    await expect(burger(page)).toHaveAttribute("aria-expanded", "false");
  });
});
