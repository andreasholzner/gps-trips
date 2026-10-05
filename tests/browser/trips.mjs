// Seeding trips for the detail-screen specs, through the real import API.
import { expect } from "./session.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const fixture = (name) =>
  readFileSync(fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url)));

/// A June morning's walk in Oslo, 08:00–09:00 UTC.
export const SAMPLE_GPX = fixture("sample.gpx");

/// The same walk as a plan: its points carry no times (US-79).
export const UNTIMED_GPX = fixture("untimed.gpx");

/// A walk with one hill (US-81): 1 km up 100 m between two flats, timed.
export const HILL_GPX = fixture("hill.gpx");

/// A paddle across a Norwegian fjord (US-76), where the suite's ground
/// fixture knows the water.
export const KAYAKING_GPX = fixture("activities/kayaking.gpx");

/// A geotagged JPEG (US-3): the fixture the server's own tests use, so the
/// EXIF path here is the real one.
export const GEOTAGGED_JPEG = fixture("geotagged.jpg");

/// Pick `value` (an activity's wire value, or "" for the first entry) in
/// the activity drop-down whose button is `#id`.
export async function chooseActivity(page, id, value) {
  await page.locator(`#${id}`).click();
  await page.locator(`#${id}-list [data-value="${value}"]`).click();
  await expect(page.locator(`#${id}-list`)).toHaveCount(0);
}

/// Import a trip through the real API and return its id, from the redirect —
/// which US-42 repointed at the SPA's own screen. Without an `activity` (its
/// wire value), the trip's activity is left unspecified; without a `gpx`, it
/// is the sample walk.
export async function importTrip(request, name, activity, gpx = SAMPLE_GPX) {
  const multipart = {
    gpx: { name: "track.gpx", mimeType: "application/gpx+xml", buffer: gpx },
    name,
  };
  if (activity) multipart.activity_type = activity;
  const response = await request.post("/api/import", { maxRedirects: 0, multipart });
  expect(response.status(), `importing ${name}`).toBe(303);
  return Number(response.headers()["location"].replace("/app/trips/", ""));
}

/// A way for one spec to import trips of its own and take them away again
/// afterwards. The suite shares one archive: a trip left behind is a row the
/// list spec did not seed and does not expect. Each trip is the calling
/// test's own, so one test's edits cannot reach another's.
export function ownTrips(test) {
  const created = [];
  test.afterAll(async ({ request }) => {
    // Best effort, and 404 is a fine outcome — a delete test removes its own.
    for (const id of created.splice(0)) {
      await request.delete(`/api/trips/${id}`);
    }
  });
  return async (request, label, activity, gpx) => {
    const id = await importTrip(
      request,
      `${label} ${Math.random().toString(36).slice(2, 8)}`,
      activity,
      gpx,
    );
    created.push(id);
    return id;
  };
}

/// Upload photos to a trip, as `[name, bytes]` pairs.
export async function addPhotos(request, id, photos) {
  for (const [name, buffer] of photos) {
    const uploaded = await request.post(`/api/trips/${id}/photos`, {
      multipart: { photos: { name, mimeType: "image/jpeg", buffer } },
    });
    expect(uploaded.status(), `seeding ${name}`).toBe(204);
  }
}
