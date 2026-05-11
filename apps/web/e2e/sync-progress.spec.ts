import { expect, test } from "@playwright/test";

import { openApp, readE2EState } from "./helpers/state";

test("manual sync shows mailbox progress until completion", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByRole("complementary").getByText(/^connected$/i)).toBeVisible();
  await expect(page.getByRole("article").first()).toBeVisible();

  const { token } = readE2EState();
  const syncResponse = page.request.post("/api/v1/mail/sync", {
    headers: { authorization: `Bearer ${token}` },
  });

  const banner = page.locator("[data-sync-banner]");
  await expect(banner).toBeVisible();
  await expect(banner).toContainText(/^Syncing \d+ of \d+ messages$/);

  const response = await syncResponse;
  expect(response.ok(), await response.text()).toBe(true);
  await expect(banner).toBeHidden({ timeout: 5_000 });
});
