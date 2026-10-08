import { expect, test } from "@playwright/test";

import { expectCursorOn, mailList, openList, reader, threadMessages } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

// The demo dataset's "Security review follow-up" thread has five messages,
// each HTML with one remote image (demo_html_body, category 4).
const MULTI_MESSAGE_THREAD = /Re: Security review follow-up, 5 messages in conversation/;

test("a long thread folds read messages, expands on o/X, and blocks remote images until M", async ({
  page,
}) => {
  const remoteRequests: string[] = [];
  await page.context().route("https://demo.mxr.local/**", async (route) => {
    remoteRequests.push(route.request().url());
    await route.fulfill({ status: 200, contentType: "image/png", body: Buffer.alloc(0) });
  });

  await openList(page, "/m/inbox");
  const row = mailList(page).getByRole("option", { name: MULTI_MESSAGE_THREAD }).first();
  // A click puts the cursor on the row; Enter opens it with the reader focused.
  await row.click();
  await page.keyboard.press("Enter");
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");

  const messages = threadMessages(page);
  await expect(messages).toHaveCount(5);
  for (let index = 0; index < 4; index += 1) {
    await expect(messages.nth(index)).toHaveAttribute("data-collapsed", "true");
  }
  await expect(messages.nth(4)).not.toHaveAttribute("data-collapsed", "true");

  // Remote content stays blocked: a placeholder, a control, and no request.
  await expect(
    reader(page).getByTestId("privacy-line").getByText("Blocked 1 remote image from mxr.local."),
  ).toBeVisible();
  await expect(reader(page).getByRole("button", { name: /^Show images/ })).toBeVisible();
  expect(remoteRequests).toEqual([]);

  // K moves to the previous message; o toggles it open.
  await page.keyboard.press("K");
  await expect(messages.nth(3)).not.toHaveAttribute("data-collapsed", "true");
  await page.keyboard.press("o");
  await expect(messages.nth(3)).toHaveAttribute("data-collapsed", "true");
  await page.keyboard.press("o");
  await expect(messages.nth(3)).not.toHaveAttribute("data-collapsed", "true");

  await page.keyboard.press(";");
  for (let index = 0; index < 5; index += 1) {
    await expect(messages.nth(index)).not.toHaveAttribute("data-collapsed", "true");
  }

  await page.keyboard.press("M");
  await expect.poll(() => remoteRequests.length).toBeGreaterThan(0);
  expect(remoteRequests.every((url) => url.endsWith("/assets/onboarding-shot.png"))).toBe(true);
  await expect(reader(page).getByTestId("privacy-line")).toHaveCount(0);
});

test("n opens the next conversation and Esc returns to the list with the cursor on it", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const ids = await mailList(page)
    .getByRole("option")
    .evaluateAll((nodes) => nodes.slice(0, 3).map((node) => node.id));

  await page.keyboard.press("j");
  await expectCursorOn(page, ids[1]!);
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  const secondUrl = page.url();
  const secondTitle = await reader(page).getAttribute("aria-label");

  await page.keyboard.press("n");
  await expect(page).not.toHaveURL(secondUrl);
  await expect(reader(page)).not.toHaveAttribute("aria-label", secondTitle!);
  // The list cursor follows the reader.
  await expectCursorOn(page, ids[2]!);

  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(reader(page)).toHaveCount(0);
  await expectCursorOn(page, ids[2]!);
  await expect(mailList(page)).toBeFocused();
});
