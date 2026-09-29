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
