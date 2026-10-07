import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Locator, type Page } from "@playwright/test";

import { mailList, openList, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

/*
 * The shared page frame (components/ModeFrame.tsx): every mode page's
 * header, teaching card and rows sit in one centred column, rows end at
 * that column's right edge, and running text keeps a readable measure.
 */

const MODE_ROUTES = ["/now", "/messages", "/todo", "/updates", "/reading", "/archive"];
const LIST_ROUTES = ["/m/inbox", "/search?q=canary"];

async function horizontalOverflow(page: Page): Promise<number> {
  return page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
}

async function box(locator: Locator) {
  const found = await locator.boundingBox();
  expect(found, "element has a box").not.toBeNull();
  return found!;
}

/**
 * Characters per line of the element's text: its width over the average
 * width of a character of that text, in its own font.
 */
async function charsPerLine(locator: Locator): Promise<number> {
  return locator.evaluate((element) => {
    const text = (element.textContent ?? "").replace(/\s+/g, " ").trim();
    const style = getComputedStyle(element);
    const context = document.createElement("canvas").getContext("2d")!;
    context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    const average = context.measureText(text).width / Math.max(1, text.length);
    return element.getBoundingClientRect().width / average;
  });
}

async function openNow(page: Page) {
  await openApp(page, "/now");
  await expect(page.getByTestId("now-row").first()).toBeVisible();
  await page.waitForLoadState("networkidle");
}

for (const width of [390, 1440, 1920, 2560]) {
  test.describe(`at ${width}px`, () => {
    test.use({ viewport: { width, height: 1000 } });

    test("mode pages and lists fit the viewport", async ({ page }) => {
      const overflowing: string[] = [];
      for (const path of [...MODE_ROUTES, ...LIST_ROUTES]) {
        await openApp(page, path);
        await expect(page.locator("#main")).toBeVisible();
        await page.waitForLoadState("networkidle");
        const overflow = await horizontalOverflow(page);
        if (overflow > 0) overflowing.push(`${path} (+${overflow}px)`);
      }
      expect(overflowing).toEqual([]);
    });

    test("Now's header, card and rows share one column, actions on its right edge", async ({
      page,
    }) => {
      await openNow(page);
      const frames = page.getByRole("region", { name: "Now", exact: true }).locator(".mode-frame");
      const header = await box(frames.first());
      const body = await box(frames.nth(1));
      expect(Math.abs(header.x - body.x)).toBeLessThanOrEqual(1);
      expect(Math.abs(header.width - body.width)).toBeLessThanOrEqual(1);

      const right = body.x + body.width;
      // The Updates card is a row with its own buttons, not a done check.
      const rows = page.getByTestId("now-row").filter({ has: page.getByTestId("now-done") });
      const count = await rows.count();
      expect(count).toBeGreaterThan(0);
      for (let at = 0; at < count; at += 1) {
        const row = rows.nth(at);
        const done = await box(row.getByTestId("now-done"));
        const rowBox = await box(row);
        // Inside the row, which is inside the column.
        expect(rowBox.x + rowBox.width).toBeLessThanOrEqual(right + 1);
        expect(done.x + done.width).toBeLessThanOrEqual(rowBox.x + rowBox.width + 1);
        // Against the row's right edge, not floating mid-row.
        expect(rowBox.x + rowBox.width - (done.x + done.width)).toBeLessThan(24);
      }

      // The teaching card's prose keeps a readable measure.
      const card = page.getByTestId("mode-card").locator("p").first();
      if (await card.count()) expect(await charsPerLine(card)).toBeLessThanOrEqual(80);
    });

    test("a mode page's header lines up with its rows", async ({ page }) => {
      await openApp(page, "/todo");
      await expect(page.getByTestId("todo-row").first()).toBeVisible();
      const frames = page.locator("#main .mode-frame");
      const header = await box(frames.first());
      const body = await box(frames.nth(1));
      expect(Math.abs(header.x - body.x)).toBeLessThanOrEqual(1);
      expect(Math.abs(header.width - body.width)).toBeLessThanOrEqual(1);
      const main = await box(page.locator("#main"));
      // Centred: equal margins either side, whatever the width.
      const leftGap = header.x - main.x;
      const rightGap = main.x + main.width - (header.x + header.width);
      expect(Math.abs(leftGap - rightGap)).toBeLessThanOrEqual(2);
    });
  });
}

test.describe("Now splits into two columns only on a wide screen", () => {
  for (const [width, split] of [
    [390, false],
    [1440, false],
    [1920, true],
    [2560, true],
  ] as const) {
    test(`${width}px: ${split ? "two columns" : "one column"}`, async ({ page }) => {
      await page.setViewportSize({ width, height: 1000 });
      await openNow(page);
      const people = await box(page.getByTestId("now-section-people"));
      const updates = await box(page.getByTestId("now-section-updates"));
      if (split) {
        expect(updates.x).toBeGreaterThan(people.x + people.width - 1);
        expect(Math.abs(updates.y - people.y)).toBeLessThan(40);
      } else {
        expect(Math.abs(updates.x - people.x)).toBeLessThanOrEqual(1);
        expect(updates.y).toBeGreaterThan(people.y);
      }
    });
  }
});

for (const width of [1920, 2560]) {
  test.describe(`reader at ${width}px`, () => {
    test.use({ viewport: { width, height: 1000 } });

    test("the thread header lines up with the messages and prose keeps its measure", async ({
      page,
    }) => {
      await openList(page, "/m/inbox");
      await page.keyboard.press("Enter");
      await expect(reader(page)).toBeVisible();
      await expect(mailList(page)).toBeVisible();
      const frames = reader(page).locator(".mode-frame");
      const header = await box(frames.first());
      const body = await box(frames.nth(1));
      expect(Math.abs(header.x - body.x)).toBeLessThanOrEqual(1);
      expect(Math.abs(header.width - body.width)).toBeLessThanOrEqual(1);

      // Plain-text bodies render as MessageText; HTML bodies in a frame.
      const text = reader(page).locator('[data-testid="message-text"]').first();
      if (await text.count()) {
        expect(await charsPerLine(text)).toBeLessThanOrEqual(80);
      }
      const iframe = reader(page).locator("iframe").first();
      if (await iframe.count()) {
        const frame = await (await iframe.elementHandle())!.contentFrame();
        const lengths = await frame!.evaluate(() => {
          const context = document.createElement("canvas").getContext("2d")!;
          return [...document.querySelectorAll("p, body > div")]
            .filter((element) => !element.closest("table"))
            .map((element) => {
              const text = (element.textContent ?? "").replace(/\s+/g, " ").trim();
              const style = getComputedStyle(element);
              context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
              const average = context.measureText(text).width / Math.max(1, text.length);
              return text.length > 60 ? element.getBoundingClientRect().width / average : 0;
            });
        });
        for (const length of lengths) expect(length).toBeLessThanOrEqual(80);
      }

      // The Reader view: plain text at the app's reading measure.
      await reader(page).getByRole("radio", { name: "Reader", exact: true }).click();
      const readerText = reader(page).getByTestId("message-text").first();
      await expect(readerText).toBeVisible();
      expect(await charsPerLine(readerText)).toBeLessThanOrEqual(80);
    });
  });
}

for (const colorScheme of ["dark", "light"] as const) {
  test.describe(`axe at 1920px, ${colorScheme}`, () => {
    test.use({ colorScheme, viewport: { width: 1920, height: 1000 } });

    for (const path of ["/now", "/todo", "/archive", "/messages"]) {
      test(`no serious or critical violations on ${path}`, async ({ page }) => {
        await openApp(page, path);
        await expect(page.locator("#main")).toBeVisible();
        await page.waitForLoadState("networkidle");
        const results = await new AxeBuilder({ page })
          .withTags(["wcag2a", "wcag2aa"])
          .exclude("iframe")
          .analyze();
        const blocking = results.violations
          .filter((violation) => violation.impact === "serious" || violation.impact === "critical")
          .flatMap((violation) => violation.nodes.map((node) => `${violation.id}: ${node.target}`));
        expect(blocking).toEqual([]);
      });
    }
  });
}
