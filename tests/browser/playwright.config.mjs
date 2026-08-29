// Browser-level tests for the Dioxus SPA — the layer that covers what the
// host-target tests structurally cannot: real user events (ADR-0012's
// 2026-08-26b amendment).
//
// These run against the *shipped artifact*: the built bundle served by Axum
// at /app, backed by a real server on a throwaway data directory. That is the
// point — a component harness would not catch, for instance, a base-URL bug
// that only appears once the bundle is served for real.
import { defineConfig } from "@playwright/test";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// The bundle is an input to this suite, not something it builds: without it
// the server answers 404 at /app and every test fails for a reason that has
// nothing to do with the test. Say so once, here.
const bundle = fileURLToPath(new URL("../../public/app/index.html", import.meta.url));
if (!existsSync(bundle)) {
  throw new Error(
    "The web bundle is missing from public/app.\n" +
      "Build and install it first:\n" +
      "  (cd crates/ui-dioxus && dx build --release --platform web)\n" +
      "  rm -rf public/app && cp -r target/dx/ui-dioxus/release/web/public public/app",
  );
}

// A fresh archive per run, never the owner's own data directory.
const dataDir = mkdtempSync(join(tmpdir(), "trip-archive-browser-"));

export default defineConfig({
  testDir: ".",
  // The suite is deliberately small; parallelism would only contend for the
  // one server and its single SQLite file.
  workers: 1,
  reporter: process.env.CI ? "list" : "line",
  use: {
    baseURL: "http://127.0.0.1:3000",
    // Timing is this layer's failure mode; a trace makes it debuggable.
    trace: "retain-on-failure",
  },
  webServer: {
    command: "cargo run --quiet --release --bin trip-archive",
    cwd: "../..",
    url: "http://127.0.0.1:3000/api/trips",
    env: { TRIP_ARCHIVE_DATA_DIR: dataDir },
    reuseExistingServer: false,
    timeout: 180_000,
  },
});
