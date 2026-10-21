// The trip-list screen driven as the owner drives it (US-41, ADR-0012's
// browser layer).
//
// Scope rule, from the amendment: this file covers only what the host-target
// tests cannot reach — real user events. Anything assertable by rendering a
// component to a string belongs in `crates/ui-dioxus`, where it runs in
// milliseconds without a browser. Each test below names the event it needs.
import { expect, signIn, test } from "./session.mjs";
import { bboxOf, panRegion } from "./map.mjs";
import { chooseActivity, importTrip, seedTripList } from "./trips.mjs";

const rows = (page) => page.locator("table tbody tr");

/// A tag no archive can already hold, so the confirm-before-create step
/// (US-33) is genuinely exercised however often this file runs.
const NEW_TAG = `summer-${Math.random().toString(36).slice(2, 8)}`;

// US-19: the archive is gated, so the browser needs a session before any of
// this can run. The seeding `request` fixture arrives with one already
// (`session.mjs`).
test.beforeEach(async ({ page }) => {
  await signIn(page.request);
});

test.beforeAll(({ request }) => seedTripList(request));

test("the list narrows as the owner types (US-13)", async ({ page }) => {
  // Needs a real `input` event: `dioxus-ssr` renders, it does not type.
  await page.goto("/app/");
  await expect(rows(page)).toHaveCount(2);

  await page.getByRole("searchbox").fill("inn");

  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Inn Valley Ride");
});

// US-52 needs the region rectangle restored "when the list is loaded again",
// which only works if the filters live in the URL. That is a property of the
// address bar and a real reload, so it can only be checked here.
test("a filtered list is in the URL and survives a reload (US-52)", async ({ page }) => {
  await page.goto("/app/");
  await page.getByRole("searchbox").fill("inn");
  await expect(rows(page)).toHaveCount(1);

  // The address bar followed the typing, without stacking a history entry
  // per keystroke: one Back leaves the filtered list behind entirely.
  await expect(page).toHaveURL(/[?&]q=inn/);

  await page.reload();

  await expect(page.getByRole("searchbox")).toHaveValue("inn");
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Inn Valley Ride");
});

// The activity filter's list opens over the map below it: Leaflet's panes
// and controls must not cover it (US-84). On a phone the list lies over the
// map's zoom control.
test("the activity filter's list opens over the map (US-84)", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 900 });
  await page.goto("/app/");
  await expect(page.locator(".leaflet-control-zoom")).toBeVisible();
  // Tiles ignore the pointer, so a hit test would see through them; make
  // everything on the map hit-testable, so the test sees what is painted.
  await page.evaluate(() =>
    document.querySelectorAll(".leaflet-container *").forEach((el) => {
      el.style.pointerEvents = "auto";
    }),
  );
  await page.locator("#filter-activity").click();

  const covered = await page.locator("#filter-activity-list").evaluate((list) =>
    [...list.querySelectorAll("li")]
      .filter((li) => {
        const box = li.getBoundingClientRect();
        const hit = document.elementFromPoint(box.left + 12, box.top + box.height / 2);
        return !li.contains(hit);
      })
      .map((li) => li.dataset.value),
  );
  expect(covered).toEqual([]);
});

test("toggling kinds keeps the active filter (US-90)", async ({ page }) => {
  // Needs a real click *and* a preserved input value across it — the one
  // criterion that is only observable once both events have happened.
  await page.goto("/app/");
  await page.getByRole("searchbox").fill("route");

  // Nothing recorded matches, so the filtered-empty state shows rather than
  // the "no trips yet" one.
  await expect(page.getByText("No trips match your filters.")).toBeVisible();

  // Planned on, alongside recorded: the planned trip is listed.
  await page.getByRole("button", { name: "Planned" }).click();
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Planned Ridge Route");

  // Recorded off: the planned trip alone, the search kept throughout.
  await page.getByRole("button", { name: "Recorded" }).click();
  await expect(rows(page)).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Planned" })).toBeDisabled();
  await expect(page.getByRole("searchbox")).toHaveValue("route");
  await expect(page).toHaveURL(/kind=planned(&|$)/);
});

// US-61: the disclosure is the one thing on this screen whose whole point is
// what happens across two events — it must survive the re-render the second
// one causes. `dioxus-ssr` dispatches neither.
test("the occasional filters open on demand and stay open (US-61)", async ({ page }) => {
  await page.goto("/app/");

  // Closed to begin with, so the table starts near the top of the screen.
  const disclosure = page.locator("details", { hasText: "More filters" }).first();
  await expect(disclosure).not.toHaveAttribute("open", /.*/);
  await expect(page.getByRole("slider", { name: "Earliest month" })).toBeHidden();

  // The controls the owner reaches for constantly are not behind it.
  await expect(page.getByRole("searchbox")).toBeVisible();
  await expect(page.getByRole("button", { name: "Recorded" })).toBeVisible();

  // Its chevron leads the summary, where it is seen, rather than trailing at
  // the far right: the words start after it.
  const indent = await disclosure.locator("summary").evaluate((summary) => {
    const range = document.createRange();
    range.selectNodeContents(summary);
    return range.getClientRects()[0].left - summary.getBoundingClientRect().left;
  });
  expect(indent).toBeGreaterThanOrEqual(16);

  await page.getByText("More filters").click();
  await expect(page.getByRole("slider", { name: "Earliest month" })).toBeVisible();

  // Still open after the list re-queries: filtering as you type re-renders
  // this whole screen, and an element rebuilt each keystroke would snap shut.
  await page.getByRole("searchbox").fill("inn");
  await expect(rows(page)).toHaveCount(1);
  await expect(page.getByRole("slider", { name: "Earliest month" })).toBeVisible();
});

// US-61: the two numbers above the table. They are read off fetches that
// land in either order, which is a race only a real browser runs.
test("the list says how many trips match and how many there are (US-61)", async ({ page }) => {
  await page.goto("/app/");
  await expect(page.getByText("2 recorded trips")).toBeVisible();

  await page.getByRole("searchbox").fill("inn");

  await expect(page.getByText("1 of 2 recorded trips")).toBeVisible();

  // Each kind on is counted, in its own noun (US-90).
  await page.getByRole("searchbox").fill("");
  await page.getByRole("button", { name: "Planned" }).click();
  await expect(page.getByText("2 recorded trips · 1 planned trip")).toBeVisible();
  await page.getByRole("button", { name: "Recorded" }).click();
  await expect(page.locator(".trip-counts")).toHaveText("1 planned trip");
});

test("choosing a tag narrows the list to trips carrying it (US-38)", async ({ page }) => {
  // Needs a real `change` event on the checkbox.
  await page.goto("/app/");
  await expect(rows(page)).toHaveCount(2);

  // The tag filter is one of the occasional ones, behind the disclosure
  // (US-61).
  await page.getByText("More filters").click();
  await page.getByRole("checkbox", { name: "alpine" }).check();

  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Oslo Hills Walk");
});

// US-63: on a phone the table scrolls inside its own box rather than being
// squeezed into it — a scrollbar is invisible to a rendered string.
test("on a phone the table scrolls sideways and the page does not (US-63)", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/app/");
  await expect(rows(page)).toHaveCount(2);

  const box = page.locator(".table-scroll");
  const overflow = await box.evaluate((el) => el.scrollWidth - el.clientWidth);
  expect(overflow, "the box has something to scroll").toBeGreaterThan(0);
  const pageOverflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(pageOverflow, "the page itself does not scroll sideways").toBeLessThanOrEqual(0);

  // A number stays on one line rather than wrapping into a column of digits.
  for (const column of [4, 5, 6, 7]) {
    await expect(rows(page).first().locator("td").nth(column)).toHaveCSS("white-space", "nowrap");
  }
});

test("Add tags the selected trips at once, a new tag after confirming it (US-34, US-112)", async ({
  page,
  request,
}) => {
  // Needs the whole click sequence: select, add, confirm. The request
  // itself is covered by the api tests in `crates/ui-dioxus`; what is only
  // reachable here is that the screen's controls actually drive it.

  // A tag that exists but no trip carries yet (US-83), so the trips it
  // ends up on are only the ones Add put it on.
  const existing = `${NEW_TAG}-old`;
  expect((await request.post("/api/tags", { data: { name: existing } })).status()).toBe(201);
  await page.goto("/app/");
  await expect(rows(page)).toHaveCount(2);

  // Select every listed trip at once, and open tagging (US-90).
  await page.locator("table thead input[type=checkbox]").check();
  await page.getByRole("button", { name: "Tag…" }).click();

  // An existing tag: Add alone tags them.
  await page.getByPlaceholder("add a tag").fill(existing);
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText(`Added "${existing}" to 2 selected.`)).toBeVisible();

  // A tag that does not exist yet must be confirmed first, so a typo cannot
  // quietly become a tag (US-33); confirming adds it too.
  await page.getByPlaceholder("add a tag").fill(NEW_TAG);
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText(`Tag "${NEW_TAG}" doesn't exist yet`)).toBeVisible();
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByText(`Added "${NEW_TAG}" to 2 selected.`)).toBeVisible();

  // The selection stays for the next tag, and both tags are filter choices
  // that list exactly the trips they were added to.
  await expect(page.locator("table tbody input[type=checkbox]:checked")).toHaveCount(2);
  await page.getByText("More filters").click();
  await page.getByRole("checkbox", { name: existing }).check();
  await page.getByRole("checkbox", { name: NEW_TAG, exact: true }).check();
  await expect(rows(page)).toHaveCount(2);
});

// US-63: one activity type for the selected trips. Choosing, confirming and
// the list catching up are real events. Its two trips are its own, removed
// again afterwards, so no other test sees their activity change.
test.describe("one activity for many trips (US-63)", () => {
  const seeded = [];

  test.beforeAll(async ({ request }) => {
    for (const name of ["Bulk Activity One", "Bulk Activity Two"]) {
      seeded.push(await importTrip(request, name, "hiking"));
    }
  });

  test.afterAll(async ({ request }) => {
    for (const id of seeded.splice(0)) {
      expect((await request.delete(`/api/trips/${id}`)).status()).toBe(204);
    }
  });

  test("the selected trips get one activity, after confirming", async ({ page }) => {
    await page.goto("/app/?q=bulk%20activity");
    await expect(rows(page)).toHaveCount(2);

    await page.locator("table thead").getByRole("checkbox").check();
    await page.getByRole("button", { name: "Set activity…" }).click();
    await chooseActivity(page, "bulk-activity", "kayaking");
    await page.getByRole("button", { name: "Set for 2 selected" }).click();

    // It says what it will overwrite, and has not done it yet.
    await expect(page.getByText("Set the activity of 2 trips to Kayaking?")).toBeVisible();
    await expect(rows(page).filter({ has: page.locator('[title="Hiking"]') })).toHaveCount(2);

    await page.getByRole("button", { name: "Change", exact: true }).click();

    await expect(rows(page).filter({ has: page.locator('[title="Kayaking"]') })).toHaveCount(2);
    // Done with: the selection is cleared, and the panels with it.
    await expect(page.getByLabel("Activity for selected trips")).toHaveCount(0);
  });
});

// US-63's paging. Moving between pages and keeping a selection across them
// are real clicks. Its 51 trips are seeded here and removed again afterwards,
// so every other test in this file keeps its two-trip archive.
test.describe("paging (US-63)", () => {
  const paged = [];

  test.beforeAll(async ({ request }) => {
    for (let n = 0; n < 51; n++) {
      paged.push(await importTrip(request, `Paged Trip ${String(n).padStart(2, "0")}`));
    }
  });

  test.afterAll(async ({ request }) => {
    for (const id of paged.splice(0)) {
      expect((await request.delete(`/api/trips/${id}`)).status()).toBe(204);
    }
  });

  test("a selection survives paging, and select-all covers one page", async ({ page }) => {
    await page.goto("/app/?q=paged");
    await expect(rows(page)).toHaveCount(50);
    await expect(page.getByText(/^51 of \d+ recorded trips · showing 1–50$/)).toBeVisible();

    await rows(page).first().getByRole("checkbox").check();
    await page.getByRole("button", { name: "Next ›" }).click();

    await expect(rows(page)).toHaveCount(1);
    await expect(page.getByText("Page 2 of 2")).toBeVisible();
    // Select-all here takes this page's one row, not the fifty behind it.
    await page.locator("table thead").getByRole("checkbox").check();
    await expect(page.getByText("2 selected: 2 trips")).toBeVisible();

    await page.getByRole("button", { name: "‹ Previous" }).click();
    await expect(rows(page).first().getByRole("checkbox")).toBeChecked();
  });

  test("changing the filters goes back to the first page", async ({ page }) => {
    await page.goto("/app/?q=paged");
    await page.getByRole("button", { name: "Next ›" }).click();
    await expect(page.getByText("Page 2 of 2")).toBeVisible();

    await page.getByRole("searchbox").fill("paged trip");

    await expect(rows(page)).toHaveCount(50);
    await expect(page.getByText("Page 1 of 2")).toBeVisible();
    // Where the owner is stays out of the URL (US-52 is about what the list is).
    await expect(page).not.toHaveURL(/page=/);
  });

  test("with Filter to map on, a pan goes back to the first page (US-92)", async ({ page }) => {
    await page.goto("/app/?q=paged");
    await expect(page.locator("#region-map .trip-line").first()).toBeVisible();
    await page.locator("#region-follow").check();
    await expect(page).toHaveURL(/[?&]bbox=/);
    await page.getByRole("button", { name: "Next ›" }).click();
    await expect(page.getByText("Page 2 of 2")).toBeVisible();

    // A small pan keeps every trip in view, but moves the region.
    const before = bboxOf(page);
    await panRegion(page, 0.4, 0.5);
    await expect.poll(() => bboxOf(page)).not.toEqual(before);

    await expect(page.getByText("Page 1 of 2")).toBeVisible();
  });
});

// US-66's "Unnamed" filter. Ticking it is a real `change` event. Its one
// properly named trip is seeded here and removed again afterwards, so every
// other test in this file keeps its two-trip archive — whose names carry no
// date, and so are exactly the trips this filter keeps.
test.describe("trips to tidy up (US-66)", () => {
  let dated;

  test.beforeAll(async ({ request }) => {
    dated = await importTrip(request, "2024-06-01 Oslo Dated Walk");
  });

  test.afterAll(async ({ request }) => {
    expect((await request.delete(`/api/trips/${dated}`)).status()).toBe(204);
  });

  test("ticking Unnamed keeps the trips without a proper name, across a reload", async ({
    page,
  }) => {
    await page.goto("/app/?q=walk");
    await expect(rows(page)).toHaveCount(2);

    await page.getByText("More filters").click();
    await page.getByRole("checkbox", { name: "Unnamed" }).check();

    await expect(rows(page)).toHaveCount(1);
    await expect(rows(page).first()).toContainText("Oslo Hills Walk");
    await expect(page).toHaveURL(/[?&]unnamed=true/);

    await page.reload();

    await expect(rows(page)).toHaveCount(1);
    await page.getByText("More filters").click();
    await expect(page.getByRole("checkbox", { name: "Unnamed" })).toBeChecked();
  });

  // Layout, which only a browser computes: where the boxes share a line with
  // the date and distance fields, they sit level with those fields' inputs,
  // not down on the margin under them.
  test("the checkboxes line up with the fields beside them", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto("/app/");
    await page.getByText("More filters").click();

    const middle = (locator) =>
      locator.evaluate((el) => {
        const box = el.getBoundingClientRect();
        return (box.top + box.bottom) / 2;
      });
    const field = await middle(page.getByLabel("Max km"));
    for (const name of ["Unnamed", "Unplaced photos"]) {
      const box = await middle(page.getByRole("checkbox", { name }));
      expect(Math.abs(box - field), name).toBeLessThanOrEqual(1);
    }
  });
});

// US-90: the bar opens one panel at a time, under it; which one is a real
// click. Layout, which only a browser computes: a panel's button sits level
// with the field beside it.
test("the selection bar opens one panel at a time", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/app/");
  await page.locator("table thead input[type=checkbox]").check();
  await expect(page.getByText("2 selected: 2 trips")).toBeVisible();

  await page.getByRole("button", { name: "Tag…" }).click();
  await expect(page.getByText("Tag selected trips")).toBeVisible();
  const middle = (locator) =>
    locator.evaluate((el) => {
      const box = el.getBoundingClientRect();
      return (box.top + box.bottom) / 2;
    });
  const entry = await middle(page.getByPlaceholder("add a tag"));
  const add = await middle(page.getByRole("button", { name: "Add", exact: true }));
  expect(Math.abs(add - entry)).toBeLessThanOrEqual(1);

  await page.getByRole("button", { name: "Set activity…" }).click();
  await expect(page.getByText("Tag selected trips")).toHaveCount(0);
  await expect(page.locator("#bulk-activity")).toBeVisible();

  // Pressed again, it closes.
  await page.getByRole("button", { name: "Set activity…" }).click();
  await expect(page.locator("#bulk-activity")).toHaveCount(0);

  await page.getByRole("button", { name: "Share…" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
});

// Layout, which only a browser computes: a confirmation's buttons stand
// apart from its question and from each other, on both panels that ask one
// (US-34, US-63). Nothing is applied: only the questions are raised.
test("the confirmations' buttons do not touch", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/app/");
  await page.locator("table thead input[type=checkbox]").check();

  // One panel open at a time (US-90): each question is raised in turn.
  const raise = {
    tag: async () => {
      await page.getByRole("button", { name: "Tag…" }).click();
      await page.getByPlaceholder("add a tag").fill(`unconfirmed-${NEW_TAG}`);
      await page.getByRole("button", { name: "Add", exact: true }).click();
    },
    activity: async () => {
      await page.getByRole("button", { name: "Set activity…" }).click();
      await chooseActivity(page, "bulk-activity", "kayaking");
      await page.getByRole("button", { name: /^Set for/ }).click();
    },
  };
  for (const [ask, confirm, cancel] of [
    [
      raise.tag,
      page.getByRole("button", { name: "Create", exact: true }),
      page.getByRole("button", { name: "Cancel" }),
    ],
    [
      raise.activity,
      page.getByRole("button", { name: "Change", exact: true }),
      page.getByRole("button", { name: "Cancel" }),
    ],
  ]) {
    await ask();
    // The question is the text before the first button.
    const question = await confirm.evaluate((button) => {
      const range = document.createRange();
      range.selectNodeContents(button.parentElement.firstChild);
      const box = range.getBoundingClientRect();
      return { right: box.right, bottom: box.bottom };
    });
    const first = await confirm.boundingBox();
    const second = await cancel.boundingBox();
    expect(second.x - (first.x + first.width), "between the buttons").toBeGreaterThanOrEqual(8);
    // Beside the question with room between them, or under it.
    const beside = first.x - question.right >= 8;
    const under = first.y >= question.bottom;
    expect(beside || under, "the question and its first button").toBe(true);
  }
});

// Layout, which only a browser computes: the region map's buttons stand
// apart rather than touching (US-63).
test("the region map's controls do not touch", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/app/");

  const follow = await page.locator(".region-controls > label").boundingBox();
  const fit = await page.locator("#region-fit").boundingBox();
  expect(fit.x - (follow.x + follow.width)).toBeGreaterThanOrEqual(8);
});

// US-90: a panel the owner left open closes with the selection, and does not
// reopen by itself with the next one — two real clicks apart.
test("an emptied selection closes the open panel for good", async ({ page }) => {
  await page.goto("/app/");
  const all = page.locator("table thead input[type=checkbox]");
  await all.check();
  await page.getByRole("button", { name: "Tag…" }).click();
  await expect(page.getByText("Tag selected trips")).toBeVisible();

  await all.uncheck();
  await expect(page.locator(".selection-bar")).toHaveCount(0);
  await all.check();

  await expect(page.locator(".selection-bar")).toBeVisible();
  await expect(page.getByText("Tag selected trips")).toHaveCount(0);
});
