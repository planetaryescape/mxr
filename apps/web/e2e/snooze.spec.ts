import { expect, test, type Page } from "@playwright/test";

import {
  cursorRowId,
  mailList,
  openList,
  pressSequence,
  rowById,
  stableRowName,
} from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

test("b snoozes to a preset; the conversation waits on /snoozed until Wake now", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const rowId = await cursorToSingleMessageRow(page);
  const subject = stableRowName((await rowById(page, rowId).getAttribute("aria-label"))!)
    .split(", ")
    .at(-1)!;

  await page.keyboard.press("b");
  const dialog = page.getByRole("dialog", { name: "Not now" });
  await expect(dialog).toBeVisible();
  await presets(page).first().click();

  await expect(dialog).toHaveCount(0);
  await expect(rowById(page, rowId)).toHaveCount(0);
  // The toast names the exact wake time the preset showed.
  await expect(page.getByText(/^Snoozed until \w+ \d+ \w+, \d\d:\d\d$/)).toBeVisible();

  await pressSequence(page, "g", "n");
  await expect(page).toHaveURL(/\/snoozed$/);
  const row = mailList(page).getByRole("option", { name: new RegExp(escapeRegExp(subject)) });
  await expect(row).toHaveCount(1);

  // Keyboard path: the cursor on the row, then w (the list's row action).
  await row.click();
  await page.keyboard.press("Escape");
  await page.keyboard.press("w");
  await expect(page.getByText("Back in your inbox")).toBeVisible();
  await expect(row).toHaveCount(0);

  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(
    mailList(page)
      .getByRole("option", { name: new RegExp(escapeRegExp(subject)) })
      .first(),
  ).toBeVisible();
});

function presets(page: Page) {
  return page
    .getByRole("dialog", { name: "Not now" })
    .getByRole("group", { name: "Times" })
    .getByRole("button");
}

async function cursorToSingleMessageRow(page: Page): Promise<string> {
  for (let step = 0; step < 15; step += 1) {
    const id = await cursorRowId(page);
    const name = (await rowById(page, id).getAttribute("aria-label")) ?? "";
    if (!/messages in conversation/.test(name)) return id;
    await page.keyboard.press("j");
  }
  throw new Error("no single-message conversation in the first rows");
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
