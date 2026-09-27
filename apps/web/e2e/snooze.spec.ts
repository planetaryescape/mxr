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

test("Z snoozes to a preset; the conversation waits on /snoozed until Wake now", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const rowId = await cursorToSingleMessageRow(page);
  const subject = stableRowName((await rowById(page, rowId).getAttribute("aria-label"))!)
    .split(", ")
    .at(-1)!;

  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
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
    .getByRole("dialog", { name: "Snooze until…" })
    .getByRole("group", { name: "Snooze presets" })
    .getByRole("button");
}

/** The row chip is mouse-only (rows are listbox options); w is its key. */
function wakeChips(page: Page) {
  return mailList(page).locator('[title^="Wake "][title$=" now (w)"]');
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

test("Z then 1 chooses the first preset", async ({ page }) => {
  await openList(page, "/m/inbox");
  const rowId = await cursorRowId(page);
  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
  await expect(presets(page).first()).toBeVisible();
  await page.keyboard.press("1");
  await expect(dialog).toHaveCount(0);
  await expect(rowById(page, rowId)).toHaveCount(0);
});

// The row's hover toolbar must not cover the Wake chip.
test("Wake now is clickable with the mouse", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Z");
  await presets(page).first().click();
  await pressSequence(page, "g", "n");
  const wake = wakeChips(page).first();
  await wake.hover();
  await wake.click({ timeout: 5_000 });
  await expect(page.getByText("Back in your inbox")).toBeVisible();
});

// /snoozed lists a conversation once, and waking it wakes every message.
test("waking a snoozed conversation wakes all of its messages", async ({ page }) => {
  await openList(page, "/m/inbox");
  const row = mailList(page)
    .getByRole("option", { name: /5 messages in conversation/ })
    .first();
  // The inbox shows the latest subject ("Re: …"); /snoozed may show another.
  const subject = stableRowName((await row.getAttribute("aria-label"))!)
    .split(", ")
    .at(-1)!
    .replace(/^(re|fwd?):\s*/i, "");
  await row.click();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Z");
  await presets(page).first().click();
  await pressSequence(page, "g", "n");
  // Other tests may leave their own snoozes; this conversation is one row.
  const chips = mailList(page).locator(`[title*="${subject} now (w)"]`);
  await expect(chips).toHaveCount(1);
  await chips.click();
  await expect(page.getByText("Back in your inbox")).toBeVisible();
  await expect(chips).toHaveCount(0);
});
