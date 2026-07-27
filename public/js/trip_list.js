// US-34: select multiple trips on the list page and bulk-apply tags to them.
//
// Row checkboxes (`.trip-select`) and a "select all" checkbox drive the
// bulk-tag panel's visibility and its "Apply to N selected" label. Typed tag
// names are staged as removable chips (not yet submitted) before "Apply"
// sends them all, for every checked trip, in one `POST /api/trips/tags`
// request — mirroring the confirm-before-creating-a-new-tag flow the detail
// page already uses for a single trip (US-33).
//
// US-38: the filter form's tag `<select multiple id="tags-select">` has no
// `name` of its own — a plain multi-select submission would produce repeated
// `tags=`/`tags=` query keys, which axum's `Query` extractor can't parse into
// a `Vec`. Instead, on submit its selected options are joined into the
// actual submitted field, the comma-separated hidden `#tags-input`.
//
// US-14: the region filter's map. `#region-input` carries the rectangle the
// owner drags out as `minLon,minLat,maxLon,maxLat` (ADR-0008); the map is
// initialised lazily, the first time the `<details>` opens, so an ordinary
// list view fetches no OSM tiles.
"use strict";

(async function () {
  wireBulkTag();
  wireTagFilter();
  wireRegionFilter();
})();

// The default view when no region is selected yet: Europe, roughly Iceland to
// Malta and Iberia to the western Urals. `fitBounds` rather than a fixed
// centre/zoom so it adapts to however wide the container actually is.
const EUROPE_BOUNDS = [
  [34.0, -12.0],
  [66.0, 34.0],
];

// Coordinate decimals kept in the query string. Six is ~10 cm — far finer than
// a rectangle dragged by hand needs, and it keeps shared URLs readable.
const BBOX_DECIMALS = 6;

function wireRegionFilter() {
  const details = document.getElementById("region-filter");
  const container = document.getElementById("region-map");
  const input = document.getElementById("region-input");
  const selectButton = document.getElementById("region-select");
  const clearButton = document.getElementById("region-clear");
  const hint = document.getElementById("region-hint");
  if (!details || !container || !input || !selectButton || !clearButton) return;

  let map = null;
  let rectangle = null;
  // Set while "Select area" is armed: the map's own dragging is disabled and
  // a mousedown starts drawing instead of panning.
  let drawing = false;
  let origin = null;

  const setHint = (text) => {
    if (hint) hint.textContent = text;
  };

  const showRectangle = (bounds) => {
    if (rectangle) {
      rectangle.setBounds(bounds);
    } else {
      rectangle = L.rectangle(bounds, { color: "#3388ff", weight: 2 }).addTo(map);
    }
  };

  // Write the rectangle into the submitted field. The corners must be brought
  // back into real-world coordinate ranges first: Leaflet's map repeats
  // horizontally, so once it has been panned onto another copy of the world it
  // reports longitudes like 309 rather than -51, and a drag into the empty
  // space above/below the projected world reports latitudes beyond ±90 —
  // either of which the server rejects with a 400 (`filter::parse_filter`).
  // `wrapLatLngBounds` shifts the whole rectangle back by whole world widths,
  // keeping its size and its position on the globe. A rectangle that genuinely
  // straddles the antimeridian still ends up out of range, and still gets the
  // server's 400 — that one is unsupported in v1 by ADR-0011, unlike merely
  // having panned east.
  const storeBounds = (bounds) => {
    const wrapped = map.wrapLatLngBounds(bounds);
    const round = (n) => Number(n.toFixed(BBOX_DECIMALS));
    const clampLat = (n) => Math.min(90, Math.max(-90, n));
    input.value = [
      round(wrapped.getWest()),
      round(clampLat(wrapped.getSouth())),
      round(wrapped.getEast()),
      round(clampLat(wrapped.getNorth())),
    ].join(",");
  };

  // Finish the rectangle at `event`'s position, clamped to the map itself: a
  // drag that runs off the edge ends at the edge, rather than at an
  // extrapolated corner the owner never saw.
  const finishAt = (event) => {
    const rect = container.getBoundingClientRect();
    const x = Math.min(Math.max(event.clientX, rect.left), rect.right);
    const y = Math.min(Math.max(event.clientY, rect.top), rect.bottom);
    const corner = map.containerPointToLatLng([x - rect.left, y - rect.top]);
    const bounds = L.latLngBounds(origin, corner);
    showRectangle(bounds);
    storeBounds(bounds);
    endDrawing();
    setHint("Area selected — press Filter to apply.");
  };

  // Leaflet must not lay out a map inside a closed <details> — the container
  // has no size there, and every tile lands in the wrong place. So build it on
  // first open, and on later opens just recompute the size.
  const ensureMap = () => {
    if (map) {
      map.invalidateSize();
      return;
    }
    map = L.map(container);
    L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
      maxZoom: 19,
      attribution: "© OpenStreetMap contributors",
    }).addTo(map);

    const selected = parseBbox(input.value);
    if (selected) {
      showRectangle(selected);
      map.fitBounds(selected, { padding: [20, 20] });
    } else {
      map.fitBounds(EUROPE_BOUNDS);
    }

    map.on("mousedown", (e) => {
      if (!drawing) return;
      origin = e.latlng;
      showRectangle([origin, origin]);
    });
    map.on("mousemove", (e) => {
      if (!drawing || !origin) return;
      showRectangle([origin, e.latlng]);
    });
    // The release is handled on the document, not the map: a mouseup outside
    // the map never reaches Leaflet, which would otherwise leave a rectangle
    // drawn on the map that the filter doesn't actually hold.
    document.addEventListener("mouseup", (e) => {
      if (!drawing || !origin) return;
      finishAt(e);
    });
  };

  const startDrawing = () => {
    drawing = true;
    origin = null;
    map.dragging.disable();
    container.style.cursor = "crosshair";
    // `region-drawing` takes the map's controls out of the hit-testing while
    // the drag is armed; without it a rectangle started over the zoom buttons
    // never reaches the map and the drag silently does nothing.
    container.classList.add("region-drawing");
    setHint("Drag on the map to select an area.");
  };

  const endDrawing = () => {
    drawing = false;
    origin = null;
    map.dragging.enable();
    container.style.cursor = "";
    container.classList.remove("region-drawing");
  };

  if (details.open) ensureMap();
  details.addEventListener("toggle", () => {
    if (details.open) ensureMap();
  });

  selectButton.addEventListener("click", () => {
    details.open = true;
    ensureMap();
    if (drawing) {
      endDrawing();
      setHint("");
    } else {
      startDrawing();
    }
  });

  clearButton.addEventListener("click", () => {
    input.value = "";
    if (rectangle) {
      rectangle.remove();
      rectangle = null;
    }
    if (map) endDrawing();
    setHint("");
  });
}

// Parse `minLon,minLat,maxLon,maxLat` (ADR-0008) into Leaflet's
// [[south, west], [north, east]]. Returns null for a blank or malformed value
// — the server is what rejects a bad bbox with a 400; the map just declines to
// draw one it can't read.
function parseBbox(value) {
  const parts = (value || "").split(",").map((s) => Number(s.trim()));
  if (parts.length !== 4 || parts.some((n) => !Number.isFinite(n))) return null;
  const [minLon, minLat, maxLon, maxLat] = parts;
  return [
    [minLat, minLon],
    [maxLat, maxLon],
  ];
}

function wireTagFilter() {
  const form = document.getElementById("filter-form");
  const select = document.getElementById("tags-select");
  const hiddenInput = document.getElementById("tags-input");
  if (!form || !select || !hiddenInput) return;

  form.addEventListener("submit", () => {
    const names = Array.from(select.selectedOptions).map((o) => o.value);
    hiddenInput.value = names.join(",");
  });
}

async function wireBulkTag() {
  const panel = document.getElementById("bulk-tag-panel");
  const selectAll = document.getElementById("select-all");
  const pendingContainer = document.getElementById("bulk-tag-pending");
  const input = document.getElementById("bulk-tag-input");
  const suggestions = document.getElementById("bulk-tag-suggestions");
  const addButton = document.getElementById("bulk-tag-add");
  const applyButton = document.getElementById("bulk-tag-apply");
  if (!panel || !pendingContainer || !input || !suggestions || !addButton || !applyButton) return;

  let allTagNames = new Set();
  const pendingNames = [];

  try {
    const response = await fetch("/api/tags");
    if (response.ok) {
      const tags = await response.json();
      allTagNames = new Set(tags.map((t) => t.name));
      tags.forEach((tag) => {
        const option = document.createElement("option");
        option.value = tag.name;
        suggestions.appendChild(option);
      });
    }
  } catch (err) {
    console.error("failed to load tags:", err);
  }

  function checkedTripIds() {
    return Array.from(document.querySelectorAll(".trip-select:checked")).map((cb) =>
      Number(cb.value)
    );
  }

  function updatePanel() {
    const count = checkedTripIds().length;
    panel.style.display = count > 0 ? "block" : "none";
    applyButton.textContent = `Apply to ${count} selected`;
  }

  document.querySelectorAll(".trip-select").forEach((cb) => {
    cb.addEventListener("change", updatePanel);
  });

  if (selectAll) {
    selectAll.addEventListener("change", () => {
      document.querySelectorAll(".trip-select").forEach((cb) => {
        cb.checked = selectAll.checked;
      });
      updatePanel();
    });
  }

  function renderPendingChips() {
    pendingContainer.innerHTML = "";
    pendingNames.forEach((name) => {
      const chip = document.createElement("span");
      chip.className = "tag-chip";
      chip.textContent = `${name} `;

      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "×";
      remove.addEventListener("click", () => {
        const idx = pendingNames.indexOf(name);
        if (idx !== -1) pendingNames.splice(idx, 1);
        renderPendingChips();
      });

      chip.appendChild(remove);
      pendingContainer.appendChild(chip);
    });
  }

  addButton.addEventListener("click", async () => {
    const raw = input.value.trim();
    if (!raw) return;
    if (pendingNames.includes(raw.toLowerCase())) {
      input.value = "";
      return;
    }
    if (!allTagNames.has(raw.toLowerCase()) && !confirm(`Tag "${raw}" doesn't exist yet — create it?`)) {
      return;
    }
    pendingNames.push(raw.toLowerCase());
    renderPendingChips();
    input.value = "";
  });

  applyButton.addEventListener("click", async () => {
    const tripIds = checkedTripIds();
    if (tripIds.length === 0 || pendingNames.length === 0) return;

    try {
      const response = await fetch("/api/trips/tags", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ trip_ids: tripIds, names: pendingNames }),
      });
      if (response.ok) {
        window.location.reload();
      } else {
        alert(`Failed to apply tags (status ${response.status}).`);
      }
    } catch (err) {
      console.error("failed to apply tags:", err);
      alert("Failed to apply tags.");
    }
  });
}
