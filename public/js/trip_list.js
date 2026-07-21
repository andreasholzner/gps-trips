// US-34: select multiple trips on the list page and bulk-apply tags to them.
//
// Row checkboxes (`.trip-select`) and a "select all" checkbox drive the
// bulk-tag panel's visibility and its "Apply to N selected" label. Typed tag
// names are staged as removable chips (not yet submitted) before "Apply"
// sends them all, for every checked trip, in one `POST /api/trips/tags`
// request — mirroring the confirm-before-creating-a-new-tag flow the detail
// page already uses for a single trip (US-33).
"use strict";

(async function () {
  wireBulkTag();
})();

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
