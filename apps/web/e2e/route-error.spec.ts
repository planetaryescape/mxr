import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("a thread route error keeps the shell visible", async ({ page }) => {
  await openApp(page, "/m/inbox/not-a-real-thread-id-12345");

  await expect(page.getByRole("complementary")).toBeVisible();
  await expect(page.getByRole("alert")).toContainText(/thread|error|unavailable/i);
});
