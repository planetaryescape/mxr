import { expect, test, type Page } from "@playwright/test";

import { mailRows, openList } from "./helpers/mail";
import { KEY_TO_PAINT_P95_MS, installKeyToPaintProbe, measureKey, p95 } from "./helpers/speed";

test.use({ viewport: { width: 1440, height: 900 } });

const PRESSES = 20;

/** Keep archives in flight: the rows leave optimistically and the daemon is untouched. */
async function holdArchives(page: Page): Promise<void> {
  await page.route("**/api/v1/mail/mutations/archive", () => new Promise<void>(() => {}));
}

for (const path of ["/desk", "/m/inbox"]) {
  test(`keydown to paint stays under budget on ${path} for j, k and e`, async ({ page }) => {
    await openList(page, path);
    expect(await mailRows(page).count()).toBeGreaterThan(PRESSES / 2);
    await installKeyToPaintProbe(page);
    await holdArchives(page);

    const j = await measureKey(page, "j", PRESSES);
    const k = await measureKey(page, "k", PRESSES);
    const e = await measureKey(page, "e", PRESSES);
    const report = { j: p95(j), k: p95(k), e: p95(e) };
    test.info().annotations.push({
      type: "key-to-paint p95 ms",
      description: `${path} ${JSON.stringify(report)}`,
    });
    console.info(`[speed] ${path} p95 ms`, report);

    expect(j).toHaveLength(PRESSES);
    expect(e).toHaveLength(PRESSES);
    for (const value of Object.values(report)) expect(value).toBeLessThan(KEY_TO_PAINT_P95_MS);
  });
}
