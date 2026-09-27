import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("a missing thread shows an error in the reader and keeps the shell", async ({ page }) => {
  await openApp(page, "/m/inbox/not-a-real-thread-id-12345");

  await expect(page.getByRole("complementary", { name: "Mailboxes" })).toBeVisible();
  await expect(page.getByText("Couldn't open this conversation")).toBeVisible();
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
  // The list beside it still works.
  await expect(page.getByTestId("mailbox-list")).toBeVisible();
});
