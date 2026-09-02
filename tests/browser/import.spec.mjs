// The import screen driven as the owner drives it (US-43/US-12, ADR-0012's
// browser layer).
//
// Scope rule, from the 2026-08-26b amendment: this file covers only what the
// host-target tests structurally cannot. Every test here needs **real user
// events** — choosing a file in a picker, and what the screen does in
// response. `dioxus-ssr` dispatches none of them, which is exactly why
// US-12's acceptance criterion (the name field arrives prefilled once the GPX
// is uploaded) can only be asserted here: it is a reaction to the picker, not
// a property of a rendered string.
//
// These are the assertions that moved off the server-rendered import form
// when US-43 deleted it: coverage transferred, it did not evaporate.
import { expect, signIn, test } from "./session.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const SAMPLE_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/sample.gpx", import.meta.url)),
);

/// A track with timestamps but no `<name>` — US-12's other prefill case.
const UNNAMED_GPX = readFileSync(
  fileURLToPath(new URL("../fixtures/unnamed.gpx", import.meta.url)),
);

const GEOTAGGED_JPEG = readFileSync(
  fileURLToPath(new URL("../fixtures/geotagged.jpg", import.meta.url)),
);

/// Both fixtures start on this date, so it is the prefix every suggestion
/// below carries.
const TRACK_DATE = "2024-06-01";

/// Every trip these tests create through the screen, so they can be taken
/// away again. The suite shares one archive: a trip left behind is a row the
/// list spec did not seed and does not expect.
const created = [];

// US-19: the archive is gated, so the browser needs a session before any of
// this can run. The seeding `request` fixture arrives with one already
// (`session.mjs`).
test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.afterAll(async ({ request }) => {
  for (const id of created.splice(0)) {
    await request.delete(`/api/trips/${id}`);
  }
});

/// Choose a GPX in the screen's own picker and wait for step two.
async function chooseGpx(page, name, buffer) {
  await page.locator("#import-gpx").setInputFiles({
    name,
    mimeType: "application/gpx+xml",
    buffer,
  });
  await expect(page.locator("#confirm-import")).toBeVisible();
}

// US-12, the acceptance criterion itself: "the name field pre-filled with a
// suggested `YYYY-mm-dd` date prefix once the GPX is uploaded". The prefill
// is the screen reacting to a file being chosen, so only a browser sees it.
test("choosing a GPX prefills the name with the track's date (US-12)", async ({ page }) => {
  await page.goto("/app/import");

  await chooseGpx(page, "track.gpx", SAMPLE_GPX);

  // The track carries a name, so the date leads and the name follows it.
  await expect(page.locator("#import-name")).toHaveValue(`${TRACK_DATE} Oslo Hills Walk`);
  // And the timezone the archive guessed from where the track starts is
  // offered rather than hidden (US-4).
  await expect(page.locator("#import-timezone")).toHaveValue("Europe/Oslo");
});

// The other half of the same criterion: with no track name to offer, the
// field still arrives with the date in it and the owner types after it.
test("a track with no name of its own still prefills the date (US-12)", async ({ page }) => {
  await page.goto("/app/import");

  await chooseGpx(page, "unnamed.gpx", UNNAMED_GPX);

  await expect(page.locator("#import-name")).toHaveValue(`${TRACK_DATE} `);
});

// US-43 end to end, through the shipped bundle: a GPX and its photos chosen
// in the real pickers, submitted with a real click, landing on the trip.
// Carries US-1 (the trip is created), US-2 (its photos are stored with it),
// US-11 and US-31 (what the owner chose is what is stored).
test("importing a GPX with photos creates the trip and lands on it (US-43)", async ({
  page,
  request,
}) => {
  await page.goto("/app/import");
  await chooseGpx(page, "track.gpx", SAMPLE_GPX);

  const name = `Imported By Hand ${Math.random().toString(36).slice(2, 8)}`;
  await page.locator("#import-name").fill(name);
  await page.locator("#import-activity").selectOption("hiking");
  await page.getByLabel("Planned").check();

  // One dialog, several files — the picker the batching behind it must not
  // change.
  await page.locator("#import-photos").setInputFiles([
    { name: "first.jpg", mimeType: "image/jpeg", buffer: GEOTAGGED_JPEG },
    { name: "second.jpg", mimeType: "image/jpeg", buffer: GEOTAGGED_JPEG },
  ]);

  await page.locator("#import-confirm").click();

  // The screen goes to the trip it just made.
  await page.waitForURL(/\/app\/trips\/\d+/);
  const id = Number(page.url().match(/\/trips\/(\d+)/)[1]);
  created.push(id);
  await expect(page.locator("#trip-name")).toHaveText(name);

  // US-2: both photos followed the trip there, batching and all.
  await expect(page.getByRole("img", { name: "first.jpg" })).toBeVisible();
  await expect(page.getByRole("img", { name: "second.jpg" })).toBeVisible();

  // And it is the archive that holds them, not just the screen.
  const trip = await (await request.get(`/api/trips/${id}`)).json();
  expect(trip.name).toBe(name);
  expect(trip.activity_type).toBe("hiking");
  const photos = await (await request.get(`/api/trips/${id}/photos`)).json();
  expect(photos.length).toBe(2);
});

// Submitting twice must not make two trips — a property worth pinning
// because the screen's recovery path would turn the second confirmation's
// 404 into a whole second import.
//
// Read this for what it is: it pins the observable behaviour, and it does
// *not* isolate the state guard in `import.rs`. It passes with that guard
// removed, because the button disables itself and because two submits in one
// tick do not reach the handler twice anyway. It would only catch a much
// grosser regression than the one the guard is written against.
test("submitting twice imports one trip, not two (US-43)", async ({ page, request }) => {
  await page.goto("/app/import");
  await chooseGpx(page, "track.gpx", SAMPLE_GPX);

  const name = `Double Submit ${Math.random().toString(36).slice(2, 8)}`;
  await page.locator("#import-name").fill(name);

  // Underneath the button's own disabled state, the way a re-fired event
  // would arrive.
  await page.evaluate(() => {
    const form = document.getElementById("confirm-import");
    form.requestSubmit();
    form.requestSubmit();
  });

  await page.waitForURL(/\/app\/trips\/\d+/);
  created.push(Number(page.url().match(/\/trips\/(\d+)/)[1]));

  const matching = await (await request.get(`/api/trips?q=${encodeURIComponent(name)}`)).json();
  expect(matching.length, "one trip, however many times Import was pressed").toBe(1);
});

// Changing your mind about the file. The archive is holding the first GPX,
// and this is the only path that hands it back — reachable only from step
// two, and only by a real click.
test("choosing a different file returns to the picker (US-12)", async ({ page }) => {
  await page.goto("/app/import");
  await chooseGpx(page, "track.gpx", SAMPLE_GPX);

  await page.locator("#import-start-over").click();

  await expect(page.locator("#import-gpx")).toBeVisible();
  await expect(page.locator("#confirm-import")).toHaveCount(0);

  // And the screen is usable again: a second choice reaches step two with the
  // new file's own suggestion.
  await chooseGpx(page, "unnamed.gpx", UNNAMED_GPX);
  await expect(page.locator("#import-name")).toHaveValue(`${TRACK_DATE} `);
});

// US-1's "invalid/empty GPX is rejected with a clear error", where the owner
// now meets it: at the picker, before naming anything. The rejection arrives
// as a response to the same real event.
test("a GPX the archive cannot read is refused at the picker (US-1)", async ({ page }) => {
  await page.goto("/app/import");

  await page.locator("#import-gpx").setInputFiles({
    name: "broken.gpx",
    mimeType: "application/gpx+xml",
    buffer: Buffer.from("not xml at all"),
  });

  // The archive's own words, and the owner is still on step one with nothing
  // to name.
  await expect(page.locator(".error")).toBeVisible();
  await expect(page.locator("#confirm-import")).toHaveCount(0);
  await expect(page.locator("#import-gpx")).toBeVisible();
});
