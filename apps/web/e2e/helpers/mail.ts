import { expect, type Locator, type Page } from "@playwright/test";

import { openApp } from "./state";

/** The mail list: a listbox whose cursor is `aria-activedescendant`. */
export function mailList(page: Page): Locator {
  return page.getByTestId("mailbox-list");
}

export function mailRows(page: Page): Locator {
  return mailList(page).getByRole("option");
}

/** The reader pane for the open conversation. */
export function reader(page: Page): Locator {
  return page.getByRole("article", { name: /^Conversation:/ });
}

export function threadMessages(page: Page): Locator {
  return reader(page).getByTestId("thread-message");
}

/** Open a list route and wait until its rows have painted. */
export async function openList(page: Page, path: string): Promise<void> {
  await openApp(page, path);
  await expect(mailRows(page).first()).toBeVisible();
}

/** DOM id of the row under the keyboard cursor. */
export async function cursorRowId(page: Page): Promise<string> {
  const id = await mailList(page).getAttribute("aria-activedescendant");
  if (!id) throw new Error("mail list has no active descendant");
  return id;
}

export async function expectCursorOn(page: Page, rowId: string): Promise<void> {
  await expect(mailList(page)).toHaveAttribute("aria-activedescendant", rowId);
}

/** DOM ids of the rows currently rendered (the list is virtualized). */
export async function renderedRowIds(page: Page): Promise<string[]> {
  return mailRows(page).evaluateAll((nodes) => nodes.map((node) => node.id));
}

export function rowById(page: Page, rowId: string): Locator {
  return page.locator(`[id="${rowId}"]`);
}

/**
 * The part of a row's accessible name that survives read/star changes:
 * "Unread., Starred., Maya, Alex, Subject, …" becomes "Maya, Alex, Subject".
 */
export function stableRowName(ariaLabel: string): string {
  const parts = ariaLabel
    .split(", ")
    .filter((part) => part !== "Unread." && part !== "Starred.");
  return parts.slice(0, -1).filter((part) => !/in conversation$|^Has attachments$/.test(part)).join(", ");
}

/** Press keys one after another, as a person typing a chord would. */
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
