import { expect, type Locator, type Page } from "@playwright/test";

import { openApp } from "./state";

export function mailList(page: Page): Locator {
  return page.getByTestId("mailbox-list");
}

export function mailRows(page: Page): Locator {
  return mailList(page).getByRole("option");
}

export function reader(page: Page): Locator {
  return page.getByRole("article", { name: /^Conversation:/ });
}

export function threadMessages(page: Page): Locator {
  return reader(page).getByTestId("thread-message");
}

export async function openList(page: Page, path: string): Promise<void> {
  await openApp(page, path);
  await expect(mailRows(page).first()).toBeVisible();
}

export async function cursorRowId(page: Page): Promise<string> {
  const id = await mailList(page).getAttribute("aria-activedescendant");
  if (!id) throw new Error("mail list has no active descendant");
  return id;
}

export async function expectCursorOn(page: Page, rowId: string): Promise<void> {
  await expect(mailList(page)).toHaveAttribute("aria-activedescendant", rowId);
}

export async function renderedRowIds(page: Page): Promise<string[]> {
  return mailRows(page).evaluateAll((nodes) => nodes.map((node) => node.id));
}

export function rowById(page: Page, rowId: string): Locator {
  return page.locator(`[id="${rowId}"]`);
}

export function stableRowName(ariaLabel: string): string {
  const parts = ariaLabel.split(", ").filter((part) => part !== "Unread." && part !== "Starred.");
  return parts
    .slice(0, -1)
    .filter((part) => !/in conversation$|^Has attachments$/.test(part))
    .join(", ");
}

export function rowStates(label: string): string[] {
  return label.split(", ").filter((part) => part === "Unread." || part === "Starred.");
}

export async function rowLabel(page: Page, rowId: string): Promise<string> {
  return (await rowById(page, rowId).getAttribute("aria-label")) ?? "";
}

export async function cursorTo(page: Page, test: (label: string) => boolean): Promise<string> {
  for (let step = 0; step < 25; step += 1) {
    const id = await cursorRowId(page);
    if (test(await rowLabel(page, id))) return id;
    await page.keyboard.press("j");
  }
  throw new Error("no matching row in the first rows");
}

export async function pressSequence(page: Page, ...keys: string[]): Promise<void> {
  for (const key of keys) await page.keyboard.press(key);
}

/**
 * The app's Mod key follows the page's reported platform, and the
 * "Desktop Chrome" device reports Windows even on a macOS host, so
 * Playwright's host-based ControlOrMeta can pick the wrong key.
 */
export async function modKey(page: Page): Promise<"Meta" | "Control"> {
  const mac = await page.evaluate(() => {
    const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
    return /mac|iphone|ipad/i.test(nav.userAgentData?.platform ?? nav.platform ?? "");
  });
  return mac ? "Meta" : "Control";
}

/**
 * Resolves once every running CSS animation and transition has finished.
 * Measuring a box before then reads a slide-in mid-flight; WebKit's frames
 * arrive later than Chromium's, so a bounding box taken right after
 * `toBeVisible` can be a few pixels off.
 */
export async function animationsSettled(page: Page): Promise<void> {
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((animation) => animation.effect?.getComputedTiming().iterations !== Infinity)
        .map((animation) => animation.finished),
    ).then(() => {}),
  );
}
