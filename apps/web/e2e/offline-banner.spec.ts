import { expect, test } from "@playwright/test";

import { openApp, restartDaemon, stopDaemon } from "./helpers/state";

test("WS offline > 30s shows sticky offline banner", async ({ page }) => {
  test.setTimeout(90_000);
  await openApp(page, "/m/inbox");
  await expect(page.getByRole("contentinfo").getByText(/^connected$/i)).toBeVisible();

  await stopDaemon();
  try {
    await expect(page.locator("[data-offline-banner]")).toBeVisible({ timeout: 32_000 });
  } finally {
    await restartDaemon();
  }

  // After 30s offline the socket's backoff has reached its 30s ceiling, so
  // the reconnect can take up to one more full interval.
  await expect(page.locator("[data-offline-banner]")).toBeHidden({ timeout: 35_000 });
});
