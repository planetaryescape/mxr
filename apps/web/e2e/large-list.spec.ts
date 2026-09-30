import { expect, test, type Page } from "@playwright/test";

import { expectCursorOn, mailList, mailRows, openList, reader } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

const ROWS = 5_000;

/**
 * The demo mailbox has about a hundred conversations; rubric 1.1 asks for
 * 5k. The inbox answer is grown from a real row, so every field the list
 * renders has the daemon's shape.
 */
test.beforeEach(async ({ page }) => {
  let templateThread = "";
  // A synthetic row opens the real conversation it was grown from.
  await page.route("**/api/v1/mail/threads/bulk-thread-*", async (route) => {
    const url = route
      .request()
      .url()
      .replace(/bulk-thread-\d+/, encodeURIComponent(templateThread));
    await route.fulfill({ response: await route.fetch({ url }) });
  });
  await page.route("**/api/v1/mail/mailbox?*", async (route) => {
    const url = new URL(route.request().url());
    if (url.searchParams.get("lens_kind") !== "inbox") return route.continue();
    const response = await route.fetch();
    const json = await response.json();
    const template = json.mailbox.groups[0].rows[0];
    templateThread = template.thread_id;
    const rows = Array.from({ length: ROWS }, (_, index) => ({
      ...template,
      id: `bulk-${index}`,
      thread_id: `bulk-thread-${index}`,
      message_ids: [`bulk-${index}`],
      subject: `Bulk message ${index + 1}`,
    }));
    json.mailbox.groups = [{ id: "bulk", label: "Earlier", rows }];
    json.mailbox.has_more = false;
    json.mailbox.next_offset = null;
    await route.fulfill({ response, json });
  });
});

/** Scripts whose work the list owns: the app's source and the virtualizer. */
const LIST_CODE = /\/src\/|react-virtual|virtual-core/;
const STEPS = 60;
/**
 * Budget for list-owned work per scroll step, in V8 block executions. The
 * fixed list runs about 20k per step (the virtualizer re-measuring rows as
 * they scroll in); an inline `getItemKey`, which rebuilt every one of the
 * 5,000 positions each frame, ran about 197k.
 */
const BLOCKS_PER_STEP = 60_000;

/**
 * A model with a cached gist for every row, answered for the rows each
 * request names, as the daemon does. Lines come in once scrolling settles
 * (the list holds at most 500), so the fling below measures the list with
 * the gist machinery live and the rows on screen at load drawn with lines.
 */
async function stubCachedGists(page: Page): Promise<() => number> {
  let requests = 0;
  await page.route("**/api/v1/platform/llm/status", (route) =>
    route.fulfill({
      json: { kind: "LlmStatus", status: { enabled: true, provider: "stub", model: "stub-7b" } },
    }),
  );
  await page.route("**/api/v1/mail/gists", async (route) => {
    requests += 1;
    await route.fulfill({
      json: {
        kind: "ThreadGists",
        batch: {
          model: "available",
          gists: (route.request().postDataJSON() as { thread_ids: string[] }).thread_ids.map(
            (thread_id) => ({
              thread_id,
              status: "ready",
              gist: "Canary stays at 5% until the dashboard is quiet.",
              ask: { summary: "confirm who owns the rollout check" },
              provenance: { model: "stub-7b", locality: "local", sources: ["this_thread"] },
              from_cache: true,
            }),
          ),
          queued: [],
          skipped: [],
        },
      },
    });
  });
  return () => requests;
}

test(`${ROWS} rows stay virtualized and scroll without re-walking the list`, ({ page }) =>
  flingThroughTheList(page, false));

test(`${ROWS} rows with gist lines stay under the same budget`, ({ page }) =>
  flingThroughTheList(page, true));

async function flingThroughTheList(page: Page, gists: boolean) {
  const gistRequests = gists ? await stubCachedGists(page) : () => 0;
  // Counting executed code, not timing it: frame gaps and CPU durations on a
  // shared CI runner measure the runner's load as much as the app (the same
  // tree passed at 45 ms and failed at 90 ms). V8's block coverage counts are
  // the same on a fast laptop and a starved runner, and work that grows with
  // the list length shows up as a count that grows tenfold. Coverage starts
  // before the app loads so every function gets its counters.
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Profiler.enable");
  await cdp.send("Profiler.startPreciseCoverage", { callCount: true, detailed: true });
  await openList(page, "/m/inbox");
  if (gists) await expect(mailRows(page).first().getByTestId("row-gist")).toBeVisible();
  // Only what fits on screen (plus overscan) is in the DOM.
  expect(await mailRows(page).count()).toBeLessThan(80);
  // Taking coverage resets the counters, so the fling is counted alone.
  await cdp.send("Profiler.takePreciseCoverage");

  const frames = await mailList(page).evaluate(async (list, steps) => {
    const deltas: number[] = [];
    let last = performance.now();
    const end = list.scrollHeight - list.clientHeight;
    // Scroll the whole list one step per frame, as a fling would.
    for (let step = 1; step <= steps; step += 1) {
      list.scrollTop = (end * step) / steps;
      await new Promise(requestAnimationFrame);
      const now = performance.now();
      deltas.push(now - last);
      last = now;
    }
    return deltas;
  }, STEPS);

  const { result } = await cdp.send("Profiler.takePreciseCoverage");
  await cdp.send("Profiler.stopPreciseCoverage");
  // React's own counts are left out: its scheduler yields more often on a
  // loaded machine, so they drift with the runner. The list's code does not.
  let blocks = 0;
  let listScripts = 0;
  let mailboxListBlocks = 0;
  for (const script of result) {
    if (!LIST_CODE.test(script.url)) continue;
    listScripts += 1;
    let scriptBlocks = 0;
    for (const fn of script.functions) for (const range of fn.ranges) scriptBlocks += range.count;
    blocks += scriptBlocks;
    if (/\/MailboxList\.tsx/.test(script.url)) mailboxListBlocks += scriptBlocks;
  }
  // The budget is only an upper bound, so a build whose script URLs stop
  // matching (bundled assets, a renamed file) would pass on zero. The list
  // module must have been counted, doing real work on each step (about 5k).
  expect(listScripts, "list-owned scripts seen by coverage").toBeGreaterThan(0);
  expect(mailboxListBlocks / STEPS, "MailboxList code executed per scroll step").toBeGreaterThan(
    1_000,
  );
  frames.sort((a, b) => a - b);
  const p95 = frames[Math.floor(frames.length * 0.95)]!;
  // Frame timing is kept as context for a failure, not as the gate.
  test.info().annotations.push({
    type: "perf",
    description: `list blocks/step ${Math.round(blocks / STEPS)}, frame p95 ${p95.toFixed(1)} ms`,
  });
  console.log(
    `large-list${gists ? " (gist lines)" : ""}: ${Math.round(blocks / STEPS)} list blocks/step, frame p95 ${p95.toFixed(1)} ms`,
  );
  expect(blocks / STEPS, "list code executed per scroll step").toBeLessThan(BLOCKS_PER_STEP);

  await expect(mailRows(page).filter({ hasText: `Bulk message ${ROWS}` })).toBeVisible();
  expect(await mailRows(page).count()).toBeLessThan(80);
  if (gists) {
    // Once the fling settles, the rows on screen get their lines; the
    // fling itself asked only at its ends, not once per frame.
    await expect(mailRows(page).last().getByTestId("row-gist")).toBeVisible();
    expect(gistRequests()).toBeLessThanOrEqual(3);
  }
}

test("returning from the reader keeps the cursor and the scroll position", async ({ page }) => {
  await openList(page, "/m/inbox");
  // Deep into the list by keyboard: half pages, then open.
  for (let step = 0; step < 40; step += 1) await page.keyboard.press("Control+d");
  const cursor = await mailList(page).getAttribute("aria-activedescendant");
  // Where the cursor row sits in the list viewport. Opening the reader
  // narrows the list and rows reflow, so raw scrollTop is not the promise;
  // the row the user was on staying put on screen is.
  const rowOffset = () =>
    page.evaluate((id) => {
      const list = document.querySelector('[data-testid="mailbox-list"]')!;
      const row = document.getElementById(id!)!;
      return row.getBoundingClientRect().top - list.getBoundingClientRect().top;
    }, cursor);
  expect(await mailList(page).evaluate((list) => list.scrollTop)).toBeGreaterThan(1000);
  const before = await rowOffset();

  await page.keyboard.press("Enter");
  await expect(reader(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(reader(page)).toHaveCount(0);

  await expectCursorOn(page, cursor!);
  await expect.poll(async () => Math.abs((await rowOffset()) - before)).toBeLessThan(80);
});
