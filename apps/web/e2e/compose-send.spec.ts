import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("send draft appears in Sent folder", async ({ page }) => {
  await openApp(page, "/compose/new");

  await page.getByLabel(/^to$/i).fill("alice@example.com");
  await page.getByLabel(/subject/i).fill("smoke-send-v1-launch");
  await page.locator(".ProseMirror").fill("body");
  const sendResponse = page.waitForResponse("**/api/v1/mail/compose/session/send");
  await page.getByRole("button", { name: /^send(?! later\b)/i }).click();
  expect((await sendResponse).ok()).toBe(true);

  await openApp(page, "/m/sent");
  await expect(page.getByText("smoke-send-v1-launch")).toBeVisible();
});

test("c launches composer and keyboard discard", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByTestId("mailbox-list")).toBeVisible();

  await page.keyboard.press("c");
  const composer = page.getByRole("dialog", { name: "New message" });
  await expect(composer).toBeVisible();
  await expect(composer.getByRole("combobox", { name: "To" })).toBeFocused();

  const body = composer.getByRole("textbox", { name: "Message body" });
  await body.fill("discard this draft");
  await expect(body).toBeFocused();

  const discardShortcut = process.platform === "darwin" ? "Meta+Backspace" : "Control+Backspace";
  await body.press(discardShortcut);
  const discardDialog = page.getByRole("alertdialog", { name: "Discard draft?" });
  await expect(discardDialog).toBeVisible();
  await discardDialog.getByRole("button", { name: /^discard$/i }).click();
  await expect(composer).not.toBeVisible();
  await expect(page).toHaveURL(/\/m\/inbox$/);
});
