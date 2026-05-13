import { expect, test } from "@playwright/test";

import { openApp, restartDaemon, stopDaemon } from "./helpers/state";

test("WS disconnect surfaces reconnecting state", async ({ page }) => {
  await openApp(page, "/m/inbox");
  const connectionPill = page.getByRole("complementary").getByText(/^connected$/i);
  await expect(connectionPill).toBeVisible();

  await stopDaemon();
  await expect(page.getByRole("complementary").getByText(/reconnecting|offline/i)).toBeVisible({
    timeout: 6_000,
  });

  await restartDaemon();
  await expect(connectionPill).toBeVisible({ timeout: 8_000 });
});
