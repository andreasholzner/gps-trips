// Driving a Leaflet map from the specs.
import { expect } from "./session.mjs";

/// The zoom levels of the tiles on the map in `#container` — Leaflet keeps
/// no other trace of its zoom in the page.
export const tileZooms = (page, container) =>
  page
    .locator(`#${container} img.leaflet-tile`)
    .evaluateAll((tiles) => [...new Set(tiles.map((t) => new URL(t.src).pathname.split("/")[1]))]);

/// Press the map's "Zoom in" or "Zoom out" button `times` times from zoom
/// `from`, each press only once the last has settled: Leaflet drops a press
/// that comes during a zoom's animation. A level's tiles appear as that
/// animation starts, after the pane is marked as animating; the mark goes
/// when the map has come to rest.
export async function zoomBy(page, container, label, times, from) {
  const step = label === "Zoom in" ? 1 : -1;
  for (let i = 1; i <= times; i++) {
    await page.locator(`#${container}`).getByRole("button", { name: label }).click();
    await expect.poll(() => tileZooms(page, container)).toContain(String(from + step * i));
    await expect(page.locator(`#${container} .leaflet-map-pane`)).not.toHaveClass(
      /leaflet-zoom-anim/,
    );
  }
}

/// A point on an SVG line itself, halfway along it, in viewport coordinates,
/// once its map is scrolled into view. Aimed at the line rather than its
/// bounding box, whose middle misses a track that curves.
export async function onTheLine(line) {
  await line.evaluate((path) =>
    path.closest(".leaflet-container").scrollIntoView({ block: "center" }),
  );
  return line.evaluate((path) => {
    const at = path
      .getPointAtLength(path.getTotalLength() / 2)
      .matrixTransform(path.getScreenCTM());
    return { x: at.x, y: at.y };
  });
}

/// Click `line` where it is drawn, with the mouse.
export async function clickLine(page, line) {
  const at = await onTheLine(line);
  await page.mouse.click(at.x, at.y);
}

/// What the map in `#container` names under the pointer (US-104): one row
/// per line or waypoint, and "…" for more.
export const hintRows = (page, container) =>
  page.locator(`#${container} .pick-hint .pick-rows > *`);

/// The menu a click where several things lie opens, or any tap (US-104):
/// its entries that open a page, and all of its rows.
export const menuEntries = (page, container) =>
  page.locator(`#${container} .pick-menu`).getByRole("button");
export const menuRows = (page, container) => page.locator(`#${container} .pick-menu .pick-row`);
