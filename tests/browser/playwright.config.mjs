// Browser-level tests for the Dioxus SPA — the layer that covers what the
// host-target tests structurally cannot: real user events, and the JS widgets
// (Leaflet, uPlot) that `document::eval` draws (ADR-0012's amendment,
// ADR-0025).
//
// These run against the *shipped artifact*: the built bundle served by Axum at
// /app, backed by a real server on a throwaway data directory. That is the
// point — a component harness would not have caught, for instance, a base-URL
// bug that only appears once the bundle is served for real.
import { defineConfig } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// A fresh archive per run, never the developer's own data directory.
const dataDir = mkdtempSync(join(tmpdir(), "trip-archive-browser-"));

export default defineConfig({
  testDir: ".",
  // The suite is deliberately small; parallelism would only contend for the
  // one server and its single SQLite file.
  workers: 1,
  reporter: process.env.CI ? "list" : "line",
  use: {
    baseURL: "http://127.0.0.1:3000",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "cargo run --quiet --bin trip-archive",
    cwd: "../..",
    url: "http://127.0.0.1:3000/api/trips",
    env: { TRIP_ARCHIVE_DATA_DIR: dataDir },
    reuseExistingServer: false,
    timeout: 180_000,
  },
});
