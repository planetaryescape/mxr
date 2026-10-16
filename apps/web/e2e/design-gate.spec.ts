import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Page } from "@playwright/test";

import { mailRows, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

/*
 * The design gate (docs/blueprint/26, D138): every main page in every theme at
 * phone, laptop and desktop widths, as screenshots attached to the run for
 * review against the concept, plus the one-frame rules a screenshot can't
 * prove: one page title per frame, the title the largest text, and the
 * frame's height fixed per width.
 *
 * MXR_DESIGN_GATE_DIR also writes each screenshot to that folder.
 */

const THEMES = ["midnight", "light", "paper", "eclipse"] as const;
const WIDTHS = [390, 1024, 1440] as const;

interface GatePage {
  name: string;
  path: string;
  /** The frame always has tabs, so it is taller. */
  tabs?: boolean;
  /** Opens the first conversation: the reader frame wraps long titles, so its height is a minimum. */
  thread?: boolean;
}

const PAGES: GatePage[] = [
  { name: "now", path: "/now" },
  { name: "messages", path: "/messages" },
  { name: "todo", path: "/todo" },
  { name: "updates", path: "/updates" },
  { name: "reading", path: "/reading", tabs: true },
  { name: "archive", path: "/archive" },
  { name: "inbox", path: "/m/inbox" },
  { name: "thread", path: "/m/inbox", thread: true },
  { name: "settings", path: "/settings/theme" },
  { name: "screener", path: "/screener", tabs: true },
];

/** Heights from docs/blueprint/27 "The page frame": 88 or 120 from 640px, shorter on phones. */
function frameHeight(width: number, tabs: boolean): number {
  if (width >= 640) return tabs ? 120 : 88;
  return tabs ? 96 : 64;
}

/** A phone shows the list or the reader, not both, so the hidden pane's frame is not the page's. */
function visibleFrames(page: Page) {
  return page.locator("#main").getByTestId("page-frame").filter({ visible: true });
}

async function openPage(page: Page, gate: GatePage): Promise<void> {
  await openApp(page, gate.path);
  await expect(page.locator("#main")).toBeVisible();
  if (gate.thread) {
    await expect(mailRows(page).first()).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(reader(page)).toBeVisible();
  }
  await expect(visibleFrames(page).first()).toBeVisible();
  // Let rows and fonts settle before the picture.
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(400);
}

/** The largest computed font size among visible text in `#main`, excluding `skip`. */
async function largestOtherText(
  page: Page,
): Promise<{ title: number; other: number; text: string }> {
  return page.evaluate(() => {
    const main = document.querySelector("#main");
    // A phone parks the list pane while a conversation is open: its h1 is not on screen.
    const title = [...(main?.querySelectorAll("h1") ?? [])].find(
      (heading) => heading.getBoundingClientRect().width > 0,
    );
    if (!main || !title) throw new Error("no page title");
    const titleSize = parseFloat(getComputedStyle(title).fontSize);
    let other = 0;
    let text = "";
    const walker = document.createTreeWalker(main, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const parent = node.parentElement;
      // Beside a reader, the list has its own frame and title: one per pane.
      if (!parent || !node.textContent?.trim() || parent.closest("h1")) continue;
      // Mail bodies are the sender's pixels, not the app's type.
      if (parent.closest("iframe, [data-testid='thread-message'], .reading-themed")) continue;
      const rect = parent.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      const size = parseFloat(getComputedStyle(parent).fontSize);
      if (size > other) {
        other = size;
        text = node.textContent.trim().slice(0, 40);
      }
    }
    return { title: titleSize, other, text };
  });
}

for (const theme of THEMES) {
  test.describe(`${theme} theme`, () => {
    test.beforeEach(async ({ page }) => {
      await page.addInitScript((chosen) => {
        localStorage.setItem(
          "mxr.uiPrefs",
          JSON.stringify({ state: { theme: chosen }, version: 6 }),
        );
      }, theme);
    });

    for (const gate of PAGES) {
      test(`${gate.name} wears one page frame at every width`, async ({ page }, testInfo) => {
        for (const width of WIDTHS) {
          await test.step(`${width}px`, async () => {
            await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
            await openPage(page, gate);
            await expect(page.locator("html")).toHaveAttribute("data-theme", theme);

            // One title per frame, and every visible frame has exactly one.
            const frames = visibleFrames(page);
            const titles = page.locator("#main h1:visible");
            const count = await frames.count();
            expect(count).toBeGreaterThan(0);
            await expect(titles).toHaveCount(count);
            for (let index = 0; index < count; index += 1) {
              await expect(frames.nth(index).locator("h1")).toHaveCount(1);
            }
            const size = width >= 1024 ? "type-display" : "type-title";
            await expect(titles.first()).toHaveClass(new RegExp(`\\b(type-title|${size})\\b`));

            // The title is the largest text on the page.
            const { title, other, text } = await largestOtherText(page);
            expect(title, `title ${title}px vs "${text}" ${other}px`).toBeGreaterThan(other);

            // The first frame is the page's own: its height is fixed per width.
            const box = await frames.first().boundingBox();
            // Settings' sections are tabs only where the side nav is hidden.
            const tabs = gate.tabs ?? (gate.name === "settings" && width < 768);
            await expect(frames.first()).toHaveAttribute("data-tabs", String(tabs));
            const expected = frameHeight(width, tabs);
            if (gate.thread) {
              expect(box?.height).toBeGreaterThanOrEqual(frameHeight(width, false));
            } else {
              // The rule under the frame is its last pixel.
              expect(Math.round(box?.height ?? 0)).toBe(expected + 1);
            }

            const body = await page.screenshot();
            const label = `${gate.name}-${theme}-${width}`;
            await testInfo.attach(label, { body, contentType: "image/png" });
            const dir = process.env.MXR_DESIGN_GATE_DIR;
            if (dir) {
              mkdirSync(dir, { recursive: true });
              writeFileSync(join(dir, `${label}.png`), body);
            }
          });
        }
      });
    }
  });
}
