import { expect, test } from "@playwright/test";

import { cursorRowId, mailList, openList, rowById } from "./helpers/mail";

// Wide enough that the list shows label chips (they hide in narrow lists).
test.use({ viewport: { width: 1440, height: 900 } });

test("l creates a new label from the picker and applies it", async ({ page }) => {
  const name = `e2e-${Date.now().toString(36)}`;
  await openList(page, "/m/inbox");
  await page.keyboard.press("j");
  const rowId = await cursorRowId(page);

  await page.keyboard.press("l");
  const dialog = page.getByRole("dialog", { name: "Labels" });
  await dialog.getByPlaceholder("Find or create a label…").fill(name);
  await dialog.getByRole("option", { name: `Create “${name}”` }).click();
  await expect(page.getByText(`Created ${name}`)).toBeVisible();
  await expect(dialog.getByRole("option", { name: `${name}: applied, changed` })).toBeVisible();
  await dialog.getByRole("button", { name: /^Apply/ }).click();

  await expect(rowById(page, rowId).getByText(name, { exact: true })).toBeVisible();
  // Labels are folded under places by default. Wait for the shell refresh
  // after Apply to list the new label before clicking it.
  await page.getByRole("button", { name: "Labels" }).click();
  const link = page
    .getByRole("navigation", { name: "Labels" })
    .getByRole("link", { name, exact: true });
  await expect(link).toBeVisible();
  await link.click();
  await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
  await expect(mailList(page).getByRole("option")).toHaveCount(1);
});
