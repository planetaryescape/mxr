import { expect, test, type Page } from "@playwright/test";

import { mailRows, openList } from "./helpers/mail";
import { openApp } from "./helpers/state";

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
  await composer(page).getByRole("button", { name: /^Send (⌘|Ctrl)/ }).click();
  await expect(page.getByText(/^Sending in \d+s/)).toBeVisible();
  await expect(page.getByText(/To alice@example\.com, from fake@example\.com/)).toBeVisible();
  expect((await sendResponse).ok()).toBe(true);

  await page.keyboard.press("g");
  await page.keyboard.press("t");
  await expect(page).toHaveURL(/\/m\/sent$/);
  await expect(mailRows(page).filter({ hasText: subject })).toHaveCount(1);
});

test("c opens the composer with the To field focused", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("c");
  await expect(composer(page)).toBeVisible();
  await expect(composer(page).getByRole("combobox", { name: "To" })).toBeFocused();
});

test("a /compose/new deep link opens the composer prefilled and steps off the URL", async ({
  page,
}) => {
  // Load the app once so the token is stored, then arrive on the deep link
  // the way an external link would (no #token fragment).
  await openList(page, "/m/inbox");
  await page.goto("/compose/new?to=bob%40example.com&subject=From%20a%20link");

  await expect(composer(page)).toBeVisible();
  await expect(composer(page).getByRole("button", { name: "Remove bob@example.com" })).toBeVisible();
  await expect(composer(page).getByRole("textbox", { name: "Subject" })).toHaveValue(
    "From a link",
  );
  await expect(page).not.toHaveURL(/\/compose\//);

  await composer(page).getByRole("button", { name: "Close composer (saves draft)" }).click();
  await expect(composer(page)).toHaveCount(0);
});

test("/compose/new with a #token fragment opens the composer", async ({ page }) => {
  await openApp(page, "/compose/new?to=bob%40example.com");
  await expect(composer(page)).toBeVisible();
  await expect(page).toHaveURL(/127\.0\.0\.1/);
});
