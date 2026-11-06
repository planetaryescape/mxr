import { expect, test, type Page } from "@playwright/test";

import { mailRows, openList } from "./helpers/mail";

function composer(page: Page) {
  return page.getByRole("dialog", { name: "New message" });
}

/** The body editor is CodeMirror in vim mode: enter insert mode, type, leave. */
async function typeBody(page: Page, text: string) {
  await composer(page).locator(".cm-content").click();
  await page.keyboard.press("i");
  await page.keyboard.type(text);
  await page.keyboard.press("Escape");
}

test("a sent message appears in Sent after the undo window", async ({ page }) => {
  test.setTimeout(60_000);
  const subject = `e2e-send-${Date.now().toString(36)}`;
  await openList(page, "/m/inbox");

  await page.keyboard.press("c");
  await expect(composer(page)).toBeVisible();
  await composer(page).getByRole("combobox", { name: "To" }).fill("alice@example.com");
  await composer(page).getByRole("textbox", { name: "Subject" }).fill(subject);
  await typeBody(page, "body from the e2e suite");

  const sendResponse = page.waitForResponse("**/api/v1/mail/compose/session/send", {
    timeout: 30_000,
  });
  await composer(page)
    .getByRole("button", { name: /^Send (⌘|Ctrl)/ })
    .click();
  await expect(page.getByText(/^Sending in \d+s/)).toBeVisible();
  await expect(page.getByText(/To alice@example\.com, from alex@demo\.mxr\.local/)).toBeVisible();
  expect((await sendResponse).ok()).toBe(true);
  // The composer owns the keyboard until the app has handled the send's
  // response and closed it; a "g" typed before that is dropped, and "t" alone
  // goes nowhere.
  await expect(composer(page)).toHaveCount(0);

  await page.keyboard.press("g");
  await page.keyboard.press("t");
  await expect(page).toHaveURL(/\/m\/sent$/);
  await expect(mailRows(page).filter({ hasText: subject })).toHaveCount(1);
});
