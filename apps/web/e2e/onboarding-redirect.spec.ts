import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("blank install lands on onboarding from any URL", async ({ page }) => {
  await page.route("**/api/v1/platform/accounts", async (route) => {
    await route.fulfill({ contentType: "application/json", json: { accounts: [] } });
  });

  for (const path of ["/", "/m/inbox", "/analytics", "/rules"]) {
    await openApp(page, path);
    await expect(page).toHaveURL(/\/onboarding$/);
  }
});
