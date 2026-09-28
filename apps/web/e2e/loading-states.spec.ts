import { expect, test, type Page } from "@playwright/test";

import { mailList, mailRows, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

interface SkeletonSpan {
  shownAt: number;
  hiddenAt: number | null;
}

/** Record every time a skeleton appears and goes, from the first paint. */
async function watchSkeletons(page: Page, testId: string): Promise<void> {
  await page.addInitScript((id) => {
    const spans: SkeletonSpan[] = [];
    (window as unknown as { __skeletons: SkeletonSpan[] }).__skeletons = spans;
    new MutationObserver(() => {
      const on = document.querySelector(`[data-testid="${id}"]`) !== null;
      const open = spans.at(-1);
      if (on && (!open || open.hiddenAt !== null))
        spans.push({ shownAt: performance.now(), hiddenAt: null });
      if (!on && open && open.hiddenAt === null) open.hiddenAt = performance.now();
    }).observe(document, { subtree: true, childList: true });
  }, testId);
}

const spans = (page: Page) =>
  page.evaluate(() => (window as unknown as { __skeletons: SkeletonSpan[] }).__skeletons);

async function delay(page: Page, pattern: string, ms: number): Promise<void> {
  await page.route(pattern, async (route) => {
    await new Promise((resolve) => setTimeout(resolve, ms));
    await route.continue();
  });
}

test("a list that loads in 100 ms never shows a skeleton", async ({ page }) => {
  await watchSkeletons(page, "list-skeleton");
  await delay(page, "**/api/v1/mail/mailbox?**", 100);
  await openApp(page, "/m/inbox");
  await expect(mailRows(page).first()).toBeVisible();
  expect(await spans(page)).toEqual([]);
});

test("a list that takes 600 ms shows its skeleton, and for at least 400 ms", async ({ page }) => {
  await watchSkeletons(page, "list-skeleton");
  await delay(page, "**/api/v1/mail/mailbox?**", 600);
  await openApp(page, "/m/inbox");
  await expect(mailRows(page).first()).toBeVisible();
  const [span, ...rest] = await spans(page);
  expect(rest).toEqual([]);
  expect(span?.hiddenAt).not.toBeNull();
  expect(span!.hiddenAt! - span!.shownAt).toBeGreaterThanOrEqual(390);
});

for (const [ms, shows] of [
  [100, false],
  [600, true],
] as const) {
  test(`opening a conversation that takes ${ms} ms ${shows ? "shows" : "never shows"} the reader skeleton`, async ({
    page,
  }) => {
    await watchSkeletons(page, "reader-skeleton");
    await openApp(page, "/m/inbox");
    await expect(mailRows(page).first()).toBeVisible();
    await delay(page, "**/api/v1/mail/threads/*", ms);
    // A row far down, so nothing has prefetched its thread.
    await mailRows(page).nth(6).click();
    await expect(reader(page)).toBeVisible({ timeout: 10_000 });
    const recorded = await spans(page);
    if (!shows) {
      expect(recorded).toEqual([]);
      return;
    }
    expect(recorded).toHaveLength(1);
    expect(recorded[0]!.hiddenAt! - recorded[0]!.shownAt).toBeGreaterThanOrEqual(390);
    await expect(mailList(page)).toBeVisible();
  });
}
