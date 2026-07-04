// Trip detail page: track on an OSM map, elevation profile, and photo gallery (US-7).
//
// The map and chart come from a single fetch of the track GeoJSON (ADR-0005/0006):
// the LineString geometry feeds Leaflet; `cumulative_distance_m` / `elevation_m`
// arrays in `properties` feed the uPlot chart. The gallery fetches the photos JSON
// and renders <img> elements served from /media/*path.
"use strict";

(async function () {
  const trackUrl = document.body.dataset.trackUrl;
  const photosUrl = document.body.dataset.photosUrl;

  // Survives past the "map" tryRender call so photo markers (loaded
  // separately, below) can be added to the same map instance (US-3).
  // `undefined` if the track never loaded — drawPhotoMarkers no-ops on that.
  let map;

  if (trackUrl) {
    let track;
    try {
      const response = await fetch(trackUrl);
      if (!response.ok) {
        console.error("failed to load track:", response.status);
      } else {
        track = await response.json();
      }
    } catch (err) {
      console.error("failed to load track:", err);
    }
    if (track) {
      // Render the two views independently so a failure in one does not blank the other.
      map = tryRender("map", () => drawMap(track));
      tryRender("elevation", () => drawElevation(track));
    }
  }

  if (photosUrl) {
    try {
      const response = await fetch(photosUrl);
      if (response.ok) {
        const photos = await response.json();
        tryRender("gallery", () => drawGallery(photos));
        tryRender("markers", () => drawPhotoMarkers(map, photos));
      }
    } catch (err) {
      console.error("failed to load photos:", err);
    }
  }

  wireDeleteButton(document.body.dataset.tripId);
})();

// US-9: wire the "Delete trip" button. Plain HTML forms cannot issue a DELETE
// request, so this fetch-based handler is the only trigger for
// `DELETE /api/trips/:id` — kept deliberately thin, no other logic lives here.
function wireDeleteButton(tripId) {
  const button = document.getElementById("delete-trip");
  if (!button || !tripId) return;

  button.addEventListener("click", async () => {
    if (!confirm("Delete this trip? This cannot be undone.")) return;
    try {
      const response = await fetch(`/api/trips/${tripId}`, { method: "DELETE" });
      if (response.status === 204) {
        window.location.href = "/";
      } else {
        alert(`Failed to delete trip (status ${response.status}).`);
      }
    } catch (err) {
      console.error("failed to delete trip:", err);
      alert("Failed to delete trip.");
    }
  });
}

function tryRender(what, render) {
  try {
    return render();
  } catch (err) {
    console.error(`failed to render ${what}:`, err);
    return undefined;
  }
}

// Render the track polyline on an OSM raster map. Keep attribution and cap
// maxZoom at 19 per OSM's tile usage policy (ADR-0005). Returns the map
// instance so photo markers (US-3) can be added to the same map.
function drawMap(track) {
  const container = document.getElementById("map");
  if (!container) return null;

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
  return map;
}

// Plot a marker for every photo that has a position: US-3's "exif" source
// today, and US-4's future "interpolated" source with no changes needed here
// — only "none" (lat/lon both null) is skipped. Uses `thumbnail_url` (US-5),
// which the server always populates — falling back to the full-size `url`
// itself when a photo has no generated thumbnail — so no null-check is needed
// here.
function drawPhotoMarkers(map, photos) {
  if (!map || !photos) return;
  photos
    .filter((p) => p.lat != null && p.lon != null)
    .forEach((p) => {
      const img = document.createElement("img");
      img.src = p.thumbnail_url;
      img.alt = p.original_name;
      img.style.maxWidth = "150px";
      L.marker([p.lat, p.lon]).addTo(map).bindPopup(img);
    });
}

// Render the photo gallery: one <img> per photo, or a "no photos" message.
// Uses `thumbnail_url` (US-5) for the same reason `drawPhotoMarkers` does.
function drawGallery(photos) {
  const container = document.getElementById("gallery");
  if (!container) return;
  if (!photos || photos.length === 0) {
    container.textContent = "No photos yet.";
    return;
  }
  photos.forEach((photo) => {
    const img = document.createElement("img");
    img.src = photo.thumbnail_url;
    img.alt = photo.original_name;
    img.style.maxHeight = "200px";
    img.style.marginRight = "0.5rem";
    container.appendChild(img);
  });
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
