import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("slash search palette opens selected suggestion", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByTestId("mailbox-list")).toBeVisible();

  await page.keyboard.press("Slash");
  await expect(page.getByRole("textbox", { name: "Search mail" })).toBeFocused();
  await page.keyboard.type("Canary");

  await expect(
    page.getByRole("button", { name: /Open Canary rollout notes from Pager Relay/ }),
  ).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");

  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
});

test("slash search palette submits raw query to search page", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByTestId("mailbox-list")).toBeVisible();

  await page.keyboard.press("Slash");
  await page.getByRole("textbox", { name: "Search mail" }).fill("zzzz-unique");
  await page.keyboard.press("Enter");

  await expect(page).toHaveURL(/\/search\?/);
  await expect(page.getByLabel("Search query")).toHaveValue("zzzz-unique");
});
