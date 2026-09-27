import { expect, test, type Page } from "@playwright/test";

import {
  cursorRowId,
  mailList,
  openList,
  pressSequence,
  rowById,
  stableRowName,
} from "./helpers/mail";

// Wide enough that the list shows label chips (they hide in narrow lists).
test.use({ viewport: { width: 1440, height: 900 } });

/** Put the cursor on the first rendered row that doesn't carry `label` yet. */
async function cursorToRowWithout(page: Page, label: string): Promise<string> {
  for (let step = 0; step < 15; step += 1) {
    const id = await cursorRowId(page);
    const chips = await rowById(page, id).getByText(label, { exact: true }).count();
    if (chips === 0) return id;
    await page.keyboard.press("j");
  }
  throw new Error(`no row without ${label} in the first rows`);
}

test("l applies an existing label and the chip shows on the row", async ({ page }) => {
  await openList(page, "/m/inbox");
  const rowId = await cursorToRowWithout(page, "Hiring");

  await page.keyboard.press("l");
  const dialog = page.getByRole("dialog", { name: "Labels" });
  await expect(dialog).toBeVisible();
  const input = dialog.getByPlaceholder("Find or create a label…");
  await expect(input).toBeFocused();

  await input.fill("Hiring");
  await page.keyboard.press("Enter");
  await expect(dialog.getByRole("option", { name: "Hiring: applied, changed" })).toBeVisible();
  await dialog.getByRole("button", { name: /^Apply/ }).click();

  await expect(dialog).toHaveCount(0);
  await expect(page.getByText(/^Labelled Hiring \d+ messages?$/)).toBeVisible();
  await expect(rowById(page, rowId).getByText("Hiring", { exact: true })).toBeVisible();
});

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

test("v moves the conversation to another label and out of Inbox", async ({ page }) => {
  await openList(page, "/m/inbox");
  await pressSequence(page, "j", "j");
  const rowId = await cursorRowId(page);
  const subject = (await rowById(page, rowId).getAttribute("aria-label"))!;

  await page.keyboard.press("v");
  const dialog = page.getByRole("dialog", { name: "Move to" });
  await expect(dialog).toBeVisible();
  await dialog.getByPlaceholder("Destination label…").fill("Travel");
  await dialog.getByRole("option", { name: "Travel" }).click();

  await expect(dialog).toHaveCount(0);
  await expect(rowById(page, rowId)).toHaveCount(0);
  await expect(page.getByText(/^Moved to Travel \d+ messages?$/)).toBeVisible();

  await page.getByRole("button", { name: "Labels" }).click();
  await page
    .getByRole("navigation", { name: "Labels" })
    .getByRole("link", { name: /^Travel/ })
    .click();
  await expect(page.getByRole("heading", { level: 1, name: "Travel" })).toBeVisible();
  const movedSubject = stableRowName(subject).split(", ").at(-1)!;
  await expect(
    mailList(page).locator(`[role="option"][aria-label*="${movedSubject}"]`).first(),
  ).toBeVisible();
});

test("u undoes a move", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("j");
  const rowId = await cursorRowId(page);
  await page.keyboard.press("v");
  const dialog = page.getByRole("dialog", { name: "Move to" });
  await dialog.getByPlaceholder("Destination label…").fill("Travel");
  await dialog.getByRole("option", { name: "Travel" }).click();
  await expect(rowById(page, rowId)).toHaveCount(0);

  await page.keyboard.press("u");
  await expect(rowById(page, rowId)).toBeVisible();
});

test("u undoes a label change", async ({ page }) => {
  await openList(page, "/m/inbox");
  const rowId = await cursorToRowWithout(page, "Hiring");
  await page.keyboard.press("l");
  const dialog = page.getByRole("dialog", { name: "Labels" });
  await dialog.getByPlaceholder("Find or create a label…").fill("Hiring");
  await page.keyboard.press("Enter");
  await dialog.getByRole("button", { name: /^Apply/ }).click();
  await expect(rowById(page, rowId).getByText("Hiring", { exact: true })).toBeVisible();

  await page.keyboard.press("u");
  await expect(rowById(page, rowId).getByText("Hiring", { exact: true })).toHaveCount(0);
});
