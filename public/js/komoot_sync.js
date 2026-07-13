// "Sync now" review page (US-22): collects the checked tour_id checkboxes
// and POSTs them as JSON — a plain HTML form can't submit repeated
// same-named checkboxes as a JSON array (ADR-0008's JSON-first API), so this
// mirrors trip_detail.js's fetch-based pattern for edit/delete.
"use strict";

(function () {
  const form = document.getElementById("sync-form");
  if (!form) return;

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const tourIds = Array.from(
      form.querySelectorAll('input[name="tour_id"]:checked')
    ).map((input) => input.value);

    try {
      const response = await fetch("/api/komoot/sync", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ tour_ids: tourIds }),
      });
      if (!response.ok) {
        alert(`Sync failed (status ${response.status}).`);
        return;
      }
      const result = await response.json();
      const params = new URLSearchParams({
        pushed: result.pushed,
        synced: result.imported,
      });
      if (result.failed_tour) {
        params.set("failed_tour", result.failed_tour);
        params.set("failed_msg", result.failed_msg || "unknown error");
        params.set("failed_phase", result.failed_phase || "pull");
      }
      window.location.href = `/komoot/sync?${params.toString()}`;
    } catch (err) {
      console.error("failed to sync with Komoot:", err);
      alert("Sync failed.");
    }
  });
})();
