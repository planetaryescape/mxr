import { expect, test, type Page } from "@playwright/test";

import { mailList, openList, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

async function horizontalOverflow(page: Page): Promise<number> {
  return page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
}

test.describe("at 900px", () => {
  test.use({ viewport: { width: 900, height: 800 } });

  test("an open thread takes the full width and Esc returns to the list", async ({ page }) => {
    await openList(page, "/m/inbox");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
    await expect(reader(page)).toBeVisible();
    await expect(mailList(page)).toBeHidden();

    const main = await page.locator("#main").boundingBox();
    const article = await reader(page).boundingBox();
    expect(article!.width).toBeGreaterThan(main!.width * 0.95);
    expect(await horizontalOverflow(page)).toBe(0);

    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/\/m\/inbox$/);
    await expect(mailList(page)).toBeVisible();
    await expect(reader(page)).toHaveCount(0);
  });
});

test.describe("at 1440px", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("list and reader sit side by side", async ({ page }) => {
    await openList(page, "/m/inbox");
    await page.keyboard.press("Enter");
    await expect(reader(page)).toBeVisible();
    await expect(mailList(page)).toBeVisible();

    const list = (await mailList(page).boundingBox())!;
    const article = (await reader(page).boundingBox())!;
    expect(list.x + list.width).toBeLessThanOrEqual(article.x + 1);
    expect(Math.abs(list.y - article.y)).toBeLessThan(120);
  });
});

const MAIN_ROUTES = [
  "/m/inbox",
  "/m/archive",
  "/m/label/work",
  "/search?q=canary",
  "/snoozed",
  "/drafts",
  "/reply-queue",
  "/rules",
  "/accounts",
  "/diagnostics",
  "/analytics/storage",
  "/settings/theme",
];

for (const width of [900, 1100, 1440]) {
  test.describe(`no horizontal scroll at ${width}px`, () => {
    test.use({ viewport: { width, height: 900 } });

    test("main routes fit the viewport", async ({ page }) => {
      const overflowing: string[] = [];
      for (const path of MAIN_ROUTES) {
        await openApp(page, path);
        await expect(page.locator("#main")).toBeVisible();
        // Let data land so the check sees the populated layout.
        await page.waitForLoadState("networkidle");
        const overflow = await horizontalOverflow(page);
        if (overflow > 0) overflowing.push(`${path} (+${overflow}px)`);
      }
      expect(overflowing).toEqual([]);
    });
  });
}
