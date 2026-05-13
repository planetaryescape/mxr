import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("archive then undo restores the message", async ({ page }) => {
  await openApp(page, "/m/inbox");

  const rows = page.getByRole("article");
  await expect(rows.first()).toBeVisible();
  const rowName = await rows.first().getAttribute("aria-label");
  expect(rowName).toBeTruthy();

  const archivedRow = page.getByRole("article", { name: rowName! });
  await archivedRow.first().hover();
  const archiveResponse = page.waitForResponse("**/api/v1/mail/mutations/archive");
  await archivedRow.first().getByLabel(/^archive$/i).click();
  const response = await archiveResponse;
  expect(response.ok(), await response.text()).toBe(true);
  const body = (await response.json()) as { result?: { mutation_id?: string } };
  expect(body.result?.mutation_id).toBeTruthy();
  await expect(archivedRow).toHaveCount(0);

  await page.getByRole("button", { name: /undo/i }).click();
  await expect(archivedRow).toBeVisible();
});
