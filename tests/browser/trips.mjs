// Seeding trips for the detail-screen specs, through the real import API.
import { expect } from "./session.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const fixture = (name) =>
  readFileSync(fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url)));

/// A June morning's walk in Oslo, 08:00–09:00 UTC.
export const SAMPLE_GPX = fixture("sample.gpx");

/// A geotagged JPEG (US-3): the fixture the server's own tests use, so the
/// EXIF path here is the real one.
export const GEOTAGGED_JPEG = fixture("geotagged.jpg");

/// Import a trip through the real API and return its id, from the redirect —
/// which US-42 repointed at the SPA's own screen.
export async function importTrip(request, name) {
  const response = await request.post("/api/import", {
    maxRedirects: 0,
    multipart: {
      gpx: { name: "track.gpx", mimeType: "application/gpx+xml", buffer: SAMPLE_GPX },
      name,
    },
  });
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
  return async (request, label) => {
    const id = await importTrip(request, `${label} ${Math.random().toString(36).slice(2, 8)}`);
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
