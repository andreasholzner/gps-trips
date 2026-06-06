// Trip detail page: draw the track on an OSM map and an elevation profile (US-7).
//
// Both views come from a single fetch of the track GeoJSON (ADR-0005/0006): the
// LineString geometry feeds Leaflet; the parallel `cumulative_distance_m` /
// `elevation_m` arrays in `properties` feed the uPlot elevation chart.
"use strict";

(async function () {
  const trackUrl = document.body.dataset.trackUrl;
  if (!trackUrl) return;

  let track;
  try {
    const response = await fetch(trackUrl);
    if (!response.ok) {
      console.error("failed to load track:", response.status);
      return;
    }
    track = await response.json();
  } catch (err) {
    // Network failure or a malformed body — leave the page as-is rather than
    // throwing an unhandled rejection.
    console.error("failed to load track:", err);
    return;
  }

  // Render the two views independently so a failure in one does not blank the
  // other (they share only the fetched data, not each other's success).
  tryRender("map", () => drawMap(track));
  tryRender("elevation", () => drawElevation(track));
})();

function tryRender(what, render) {
  try {
    render();
  } catch (err) {
    console.error(`failed to render ${what}:`, err);
  }
}

// Render the track polyline on an OSM raster map. Keep attribution and cap
// maxZoom at 19 per OSM's tile usage policy (ADR-0005).
function drawMap(track) {
  const container = document.getElementById("map");
  if (!container) return;

  const map = L.map(container);
  L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
    maxZoom: 19,
    attribution: "© OpenStreetMap contributors",
  }).addTo(map);

  const line = L.geoJSON(track).addTo(map);
  const bounds = line.getBounds();
  if (bounds.isValid()) {
    map.fitBounds(bounds);
  }
}

// Render elevation (m) against cumulative distance (km) as a uPlot line chart.
function drawElevation(track) {
  const container = document.getElementById("elevation");
  if (!container) return;

  const props = track.properties || {};
  const distanceKm = (props.cumulative_distance_m || []).map((m) => m / 1000);
  const elevation = props.elevation_m || [];
  // Need a non-empty, parallel pair of series for a meaningful chart.
  if (distanceKm.length === 0 || elevation.length !== distanceKm.length) return;

  const options = {
    width: container.clientWidth || 600,
    height: 200,
    scales: { x: { time: false } },
    series: [
      { label: "Distance (km)" },
      { label: "Elevation (m)", stroke: "#3367d6", width: 2 },
    ],
    axes: [{ label: "Distance (km)" }, { label: "Elevation (m)" }],
  };
  new uPlot(options, [distanceKm, elevation], container);
}
