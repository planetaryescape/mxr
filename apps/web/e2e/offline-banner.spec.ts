import { expect, test } from "@playwright/test";

import { openApp, restartDaemon, stopDaemon } from "./helpers/state";

test("WS offline > 30s shows sticky offline banner", async ({ page }) => {
  test.setTimeout(45_000);
  await openApp(page, "/m/inbox");
  await expect(page.getByRole("complementary").getByText(/^connected$/i)).toBeVisible();

  await stopDaemon();
  try {
    await expect(page.locator("[data-offline-banner]")).toBeVisible({ timeout: 32_000 });
  } finally {
    await restartDaemon();
  }

  await expect(page.locator("[data-offline-banner]")).toBeHidden({ timeout: 5_000 });
});
