import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("401 routes to /settings/token with a visible alert", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByText(/loaded · j\/k nav/)).toBeVisible();

  await page.route("**/api/v1/auth/local-token", (route) => route.fulfill({ status: 404 }));
  await page.evaluate(() => localStorage.setItem("mxr.bridgeToken", "invalid-token"));
  await page.reload();

  await expect(page).toHaveURL(/\/settings\/token\?reason=expired/);
  await expect(page.getByRole("alert")).toContainText(/token/i);
});
