import { expect, test, type Page } from "@playwright/test";

import { mailList, mailRows, openList, reader, threadMessages } from "./helpers/mail";
import { KEY_TO_PAINT_P95_MS, installKeyToPaintProbe, measureKey, p95 } from "./helpers/speed";

test.use({ viewport: { width: 1440, height: 900 } });

/** Presses per key, capped by the rows on screen: `j` past the last row changes nothing. */
const PRESSES = 20;
const MIN_PRESSES = 10;

/** Keep mutations in flight: the rows change optimistically and the daemon is untouched. */
async function holdMutations(page: Page): Promise<void> {
  const hold = () => new Promise<void>(() => {});
  await page.route("**/api/v1/mail/mutations/archive", hold);
  await page.route("**/api/v1/mail/mutations/star", hold);
  // On the desk, e is Done.
  await page.route("**/api/v1/mail/desk/done", hold);
}

function report(path: string, samples: Record<string, number[]>): Record<string, number> {
  const result = Object.fromEntries(
    Object.entries(samples).map(([key, values]) => [key, p95(values)]),
  );
  test.info().annotations.push({
    type: "key-to-paint p95 ms",
    description: `${path} ${JSON.stringify(result)}`,
  });
  console.info(`[speed] ${path} p95 ms`, result);
  return result;
}

for (const path of ["/desk", "/m/inbox"]) {
  test(`keydown to paint stays under budget on ${path} for j, k, s and e`, async ({ page }) => {
    await openList(page, path);
    const presses = Math.min(PRESSES, (await mailRows(page).count()) - 1);
    expect(presses).toBeGreaterThanOrEqual(MIN_PRESSES);
    await installKeyToPaintProbe(page);
    await holdMutations(page);

    const samples = {
      j: await measureKey(page, "j", presses, { change: "cursor-moves" }),
      k: await measureKey(page, "k", presses, { change: "cursor-moves" }),
      s: await measureKey(page, "s", presses, { change: "star-toggles" }),
      e: await measureKey(page, "e", presses, { change: "row-leaves" }),
    };
    const result = report(path, samples);

    for (const value of Object.values(result)) expect(value).toBeLessThan(KEY_TO_PAINT_P95_MS);
  });
}

test("opening a cached conversation paints under budget", async ({ page }) => {
  await openList(page, "/m/inbox");
  await mailList(page).focus();
  // Open and close each of the first rows once, so their threads are cached.
  for (let row = 0; row < PRESSES; row += 1) {
    await page.keyboard.press("Enter");
    await expect(threadMessages(page).first()).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(reader(page)).toHaveCount(0);
    await page.keyboard.press("j");
  }
  for (let row = 0; row < PRESSES; row += 1) await page.keyboard.press("k");
  await installKeyToPaintProbe(page);

  const enter = await measureKey(page, "Enter", PRESSES, {
    change: "thread-opens",
    between: async () => {
      await page.keyboard.press("Escape");
      await expect(reader(page)).toHaveCount(0);
      await page.keyboard.press("j");
    },
  });
  const result = report("/m/inbox open", { Enter: enter });

  expect(result.Enter).toBeLessThan(KEY_TO_PAINT_P95_MS);
});

test("a key that doesn't paint its change fails the probe instead of timing an empty frame", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  await installKeyToPaintProbe(page);
  await holdMutations(page);
  // s stars the row; it never moves the cursor.
  await expect(measureKey(page, "s", 1, { change: "cursor-moves" })).rejects.toThrow(
    /1 of 1 presses never painted "cursor-moves"/,
  );
});
