// The elevation profile in either colour scheme (US-7, ADR-0012's browser
// layer). uPlot draws its axes into a canvas, in colours it is handed rather
// than ones CSS can reach, so whether its labels can be read against the
// page is only observable here — the second exemption, rendering done
// through `document::eval`. Apart from `trip_detail.spec.mjs` only to keep
// that file within the size cap.
import { expect, signIn, test } from "./session.mjs";
import { ownTrips } from "./trips.mjs";

const ownTrip = ownTrips(test);

test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

/// How the chart's axis labels contrast with the page behind them, as the
/// WCAG contrast ratio (1 = invisible, 21 = black on white), plus the
/// canvas as drawn — to tell a repaint from a stale picture.
const axisContrast = (page) =>
  page.evaluate(() => {
    const chart = window.tripArchiveWidgets.elevation;
    // Any CSS colour to [r, g, b], by letting a canvas parse it.
    const rgb = (colour) => {
      const ctx = document.createElement("canvas").getContext("2d");
      ctx.fillStyle = colour;
      ctx.fillRect(0, 0, 1, 1);
      return [...ctx.getImageData(0, 0, 1, 1).data.slice(0, 3)];
    };
    const luminance = (colour) => {
      const [r, g, b] = rgb(colour).map((c) => {
        const s = c / 255;
        return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
      });
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    const ratio = (a, b) => {
      const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
      return (hi + 0.05) / (lo + 0.05);
    };
    // The first painted background behind the chart — Pico paints the
    // page's on `<html>`, not `<body>`.
    let background = "#fff";
    for (let el = document.getElementById("elevation"); el; el = el.parentElement) {
      const colour = getComputedStyle(el).backgroundColor;
      if (!/^rgba\(.*,\s*0\)$|^transparent$/.test(colour)) {
        background = colour;
        break;
      }
    }
    const labels = chart.axes.map((axis, i) => axis.stroke(chart, i));
    return {
      contrast: Math.min(...labels.map((label) => ratio(label, background))),
      picture: document.querySelector("#elevation canvas").toDataURL(),
    };
  });

test("the elevation profile's labels can be read in either colour scheme (US-7)", async ({
  page,
  request,
}) => {
  const id = await ownTrip(request, "Themed Trip");
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto(`/app/trips/${id}`);
  await expect(page.locator("#elevation canvas")).toBeVisible();

  // 4.5 is WCAG's minimum for text; black labels on the dark page are ~2.
  const dark = await axisContrast(page);
  expect(dark.contrast).toBeGreaterThanOrEqual(4.5);

  // Switching the scheme while the chart is open repaints it in the new
  // colours, rather than leaving the old ones on the new background.
  await page.emulateMedia({ colorScheme: "light" });
  await expect.poll(async () => (await axisContrast(page)).picture).not.toBe(dark.picture);
  expect((await axisContrast(page)).contrast).toBeGreaterThanOrEqual(4.5);
});
