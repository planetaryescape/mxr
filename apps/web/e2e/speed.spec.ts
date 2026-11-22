import { expect, test, type Page } from "@playwright/test";

import { mailList, mailRows, openList, reader, threadMessages } from "./helpers/mail";
import { KEY_TO_PAINT_P95_MS, installKeyToPaintProbe, measureKey, p95 } from "./helpers/speed";
import { freshWaitingConversations } from "./helpers/waiting-request";

test.use({ viewport: { width: 1440, height: 900 } });

const PRESSES = 20;
const MIN_PRESSES = 10;

type KeyPaintSamples = {
  j: number[];
  k: number[];
  s: number[];
  e?: number[];
};

async function holdMutations(page: Page): Promise<void> {
  const hold = () => new Promise<void>(() => {});
  await page.route("**/api/v1/mail/mutations/star", hold);
  await page.route("**/api/v1/mail/desk/done", hold);
}

function report(
  path: string,
  samples: Record<string, number[] | undefined>,
): Record<string, number> {
  const result = Object.fromEntries(
    Object.entries(samples).flatMap(([key, values]) => (values ? [[key, p95(values)]] : [])),
  );
  test.info().annotations.push({
    type: "key-to-paint p95 ms",
    description: `${path} ${JSON.stringify(result)}`,
  });
  console.info(`[speed] ${path} p95 ms`, result);
  return result;
}

for (const path of ["/desk", "/m/inbox"]) {
  test(`keydown to paint stays under budget on ${path} for j, k, s${path === "/desk" ? ", and e" : ""}`, async ({
    page,
  }) => {
    const waiting = path === "/desk" ? await freshWaitingConversations(page, 4) : null;
    try {
      await openList(page, path);
      const presses = Math.min(PRESSES, (await mailRows(page).count()) - 1);
      const minPresses = path === "/desk" ? 4 : MIN_PRESSES;
      expect(presses).toBeGreaterThanOrEqual(minPresses);
      await installKeyToPaintProbe(page);
      await holdMutations(page);

      const samples: KeyPaintSamples = {
        j: await measureKey(page, "j", presses, { change: "cursor-moves" }),
        k: await measureKey(page, "k", presses, { change: "cursor-moves" }),
        s: await measureKey(page, "s", presses, { change: "star-toggles" }),
      };
      if (path === "/desk")
        samples.e = await measureKey(page, "e", presses, { change: "row-leaves" });
      const result = report(path, samples);

      for (const value of Object.values(result)) expect(value).toBeLessThan(KEY_TO_PAINT_P95_MS);
    } finally {
      await waiting?.finish();
    }
  });
}

test("opening a cached conversation paints under budget", async ({ page }) => {
  await openList(page, "/m/inbox");
  await mailList(page).focus();
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
