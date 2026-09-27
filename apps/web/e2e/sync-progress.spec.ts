import { expect, test } from "@playwright/test";

import { openList } from "./helpers/mail";

test("Sync now shows progress in the status bar until the sync completes", async ({ page }) => {
  await openList(page, "/m/inbox");
  const statusBar = page.getByRole("contentinfo");
  await expect(statusBar.getByText(/^connected$/i)).toBeVisible();

  const syncResponse = page.waitForResponse("**/api/v1/mail/sync");
  await statusBar.getByRole("button", { name: "Sync now" }).click();

  const progress = statusBar.getByRole("status").filter({ hasText: /^syncing/ });
  await expect(progress).toBeVisible();
  await expect(progress).toHaveText(/^syncing (…|\d+\/\d+ messages?)/);

  const response = await syncResponse;
  expect(response.ok(), await response.text()).toBe(true);
  await expect(progress).toBeHidden({ timeout: 10_000 });
  await expect(statusBar.getByRole("button", { name: "Sync now" })).toBeVisible();
});
