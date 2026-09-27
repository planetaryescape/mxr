import { expect, test } from "@playwright/test";

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
    const url = route.request().url().replace(/bulk-thread-\d+/, encodeURIComponent(templateThread));
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

test(`${ROWS} rows stay virtualized and scroll without long frames`, async ({ page }) => {
  await openList(page, "/m/inbox");
  // Only what fits on screen (plus overscan) is in the DOM.
  expect(await mailRows(page).count()).toBeLessThan(80);

  const frames = await mailList(page).evaluate(async (list) => {
    const deltas: number[] = [];
    let last = performance.now();
    const end = list.scrollHeight - list.clientHeight;
    // Scroll the whole list in 60 steps, one per frame, as a fling would.
    for (let step = 1; step <= 60; step += 1) {
      list.scrollTop = (end * step) / 60;
      await new Promise(requestAnimationFrame);
      const now = performance.now();
      deltas.push(now - last);
      last = now;
    }
    return deltas;
  });
  frames.sort((a, b) => a - b);
  const p95 = frames[Math.floor(frames.length * 0.95)]!;
  // A dropped frame or two is noise on a shared runner; a list that renders
  // every row shows up as frames in the hundreds of milliseconds.
  expect(p95).toBeLessThan(50);
  await expect(mailRows(page).filter({ hasText: `Bulk message ${ROWS}` })).toBeVisible();
  expect(await mailRows(page).count()).toBeLessThan(80);
});

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
