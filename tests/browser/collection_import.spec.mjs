// Adding files to a collection, and making one of several (US-88), driven
// as the owner drives it.
//
// Scope rule, from ADR-0012's 2026-08-26b amendment: **real user events** —
// choosing several files in the picker, a collection to add them to, and an
// action that the archive then proposes again. What the review renders is
// asserted in `crates/ui-dioxus/src/import/review_tests.rs`, and what the
// archive matches and stores in `tests/it/us88_add_to_collection.rs`.
import { expect, signIn, test } from "./session.mjs";

/// A guide book's file: each track a line eastwards along its latitude.
function book(tracks, waypoints = []) {
  const wpts = waypoints
    .map(([name, lat]) => `<wpt lat="${lat}" lon="9.2"><name>${name}</name></wpt>`)
    .join("\n");
  const trks = tracks
    .map(([name, lat]) => {
      const points = Array.from(
        { length: 10 },
        (_, i) => `<trkpt lat="${lat}" lon="${(9.2 + i * 0.001).toFixed(3)}"></trkpt>`,
      ).join("");
      return `<trk><name>${name}</name><trkseg>${points}</trkseg></trk>`;
    })
    .join("\n");
  return Buffer.from(
    `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
${wpts}
${trks}
</gpx>`,
  );
}

const gpxFile = (name, buffer) => ({ name, mimeType: "application/gpx+xml", buffer });

/// Every collection these tests create, taken away again afterwards: the
/// suite shares one archive.
const created = [];

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  for (const id of created.splice(0)) {
    await request.delete(`/api/collections/${id}`);
  }
});

const collectionId = (page) => Number(page.url().match(/\/collections\/(\d+)/)[1]);

test("several files make one collection, a hut in both added once (US-88)", async ({
  page,
  request,
}) => {
  await page.goto("/app/import");
  await page.getByLabel("Collection", { exact: true }).check();
  await expect(page.locator("#import-target")).toHaveValue("");
  await page
    .locator("#import-gpx")
    .setInputFiles([
      gpxFile("part-2.gpx", book([["Stage 2", 46.6]], [["Hut", 46.5]])),
      gpxFile("part-1.gpx", book([["Stage 1", 46.5]], [["Hut", 46.5]])),
    ]);

  await expect(page.locator("#review-files")).toHaveText("From part-1.gpx, part-2.gpx.");
  await expect(page.locator("#collection-name-field")).toHaveValue("part-1");
  await expect(page.locator("#waypoint-1-0")).toHaveValue("skip");
  const name = `Parts ${Math.random().toString(36).slice(2, 8)}`;
  await page.locator("#collection-name-field").fill(name);
  await page.locator("#import-confirm").click();

  await page.waitForURL(/\/app\/collections\/\d+$/);
  const id = collectionId(page);
  created.push(id);
  await expect(page.locator("#collection-name")).toHaveText(name);
  await expect(page.locator("#collection-tracks tbody tr")).toHaveCount(2);
  await expect(page.locator("#collection-counts")).toHaveText("2 tracks · 1 waypoint");
  await expect(page.locator("#collection-files li")).toHaveText(["part-1.gpx", "part-2.gpx"]);

  // A kept file downloads as it was uploaded.
  const files = await (await request.get(`/api/collections/${id}/files`)).json();
  const download = await request.get(`/api/collection-files/${files[0].id}/gpx`);
  expect(await download.text()).toContain("<name>Stage 1</name>");
});

test("a corrected file is reviewed and the owner's choice is proposed again (US-88)", async ({
  page,
  request,
}) => {
  const name = `Lakes ${Math.random().toString(36).slice(2, 8)}`;
  const response = await request.post("/api/collections", {
    multipart: {
      gpx: gpxFile(
        "first.gpx",
        book([
          ["Stage 1", 46.5],
          ["Stage 2", 46.6],
          ["Stage 3", 46.7],
        ]),
      ),
      name,
    },
  });
  expect(response.status()).toBe(201);
  const { id } = await response.json();
  created.push(id);

  await page.goto("/app/import");
  await page.getByLabel("Collection", { exact: true }).check();
  await page.locator("#import-target").selectOption({ label: name });
  // Stage 1 the same, Stage 2 changed, Stage 4 new; Stage 3 in the collection only.
  await page.locator("#import-gpx").setInputFiles([
    gpxFile(
      "corrected.gpx",
      book([
        ["Stage 1", 46.5],
        ["Stage 2", 46.61],
        ["Stage 4", 46.8],
      ]),
    ),
  ]);

  await expect(page.locator("#track-0-0")).toHaveValue("skip");
  await expect(page.locator("#track-0-1")).toHaveValue("replace");
  await expect(page.locator("#track-0-2")).toHaveValue("add");
  const stage3 = page.locator("#review-held-tracks select");
  await expect(stage3).toHaveValue("keep");

  // The owner's choice goes to the archive, which proposes again.
  await stage3.selectOption("remove");
  await expect(stage3).toHaveValue("remove");
  await page.locator("#track-0-2").selectOption("skip");
  await expect(page.locator("#track-0-2")).toHaveValue("skip");
  await expect(page.locator("#import-confirm")).toBeEnabled();
  await page.locator("#import-confirm").click();

  await page.waitForURL(new RegExp(`/app/collections/${id}$`));
  await expect(page.locator("#collection-tracks tbody tr")).toHaveText([/Stage 1/, /Stage 2/]);
  await expect(page.locator("#collection-files li")).toHaveText(["first.gpx", "corrected.gpx"]);
});

test("cancelling a review changes nothing (US-88)", async ({ page, request }) => {
  const name = `Lakes ${Math.random().toString(36).slice(2, 8)}`;
  const response = await request.post("/api/collections", {
    multipart: { gpx: gpxFile("first.gpx", book([["Stage 1", 46.5]])), name },
  });
  const { id } = await response.json();
  created.push(id);

  await page.goto("/app/import");
  await page.getByLabel("Collection", { exact: true }).check();
  await page.locator("#import-target").selectOption({ label: name });
  await page.locator("#import-gpx").setInputFiles([gpxFile("more.gpx", book([["Stage 9", 47]]))]);
  await expect(page.locator("#track-0-0")).toHaveValue("add");
  await page.locator("#import-cancel").click();

  await expect(page.locator("#import-gpx")).toBeVisible();
  const stored = await (await request.get(`/api/collections/${id}`)).json();
  expect(stored.tracks.map((t) => t.name)).toEqual(["Stage 1"]);
});
