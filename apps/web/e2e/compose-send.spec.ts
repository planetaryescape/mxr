import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("send draft appears in Sent folder", async ({ page }) => {
  await openApp(page, "/compose/new");

  await page.getByLabel(/^to$/i).fill("alice@example.com");
  await page.getByLabel(/subject/i).fill("smoke-send-v1-launch");
  await page.locator(".ProseMirror").fill("body");
  await page.getByRole("button", { name: /^send$/i }).click();
  const sendResponse = page.waitForResponse("**/api/v1/mail/compose/session/send");
  await page.getByRole("dialog").getByRole("button", { name: /^send$/i }).click();
  expect((await sendResponse).ok()).toBe(true);

  await openApp(page, "/m/sent");
  await expect(page.getByText("smoke-send-v1-launch")).toBeVisible();
});

test("c launches compose with prefilled fields and keyboard discard", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByTestId("mailbox-list")).toBeVisible();

  await page.keyboard.press("c");
  await expect(page.getByRole("dialog", { name: "Compose message" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Recipients" })).toBeFocused();

  await page.keyboard.type("qa@example.com");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("textbox", { name: "Subject" })).toBeFocused();

  await page.keyboard.type("keyboard compose launcher");
  await page.keyboard.press("Enter");

  await expect(page).toHaveURL(/\/compose\/new/);
  await expect(page.getByLabel(/^to$/i)).toHaveValue("qa@example.com");
  await expect(page.locator("#compose-subject")).toHaveValue("keyboard compose launcher");
  await expect(page.getByRole("textbox", { name: "Message body" })).toBeFocused();

  const discardShortcut = process.platform === "darwin" ? "Meta+Backspace" : "Control+Backspace";
  await page.getByRole("textbox", { name: "Message body" }).press(discardShortcut);
  const discardDialog = page.getByRole("dialog", { name: "Discard draft?" });
  await expect(discardDialog).toBeVisible();
  await discardDialog.getByRole("button", { name: /^discard$/i }).click();
  await expect(page).toHaveURL(/\/m\/inbox$/);
});
