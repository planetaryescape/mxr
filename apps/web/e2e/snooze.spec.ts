import { expect, test, type Page } from "@playwright/test";

import { cursorRowId, mailList, openList, pressSequence, rowById, stableRowName } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

test("Z snoozes to a preset; the conversation waits on /snoozed until Wake now", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  // A single-message conversation: waking multi-message ones is a known bug.
  const rowId = await cursorToSingleMessageRow(page);
  const subject = stableRowName((await rowById(page, rowId).getAttribute("aria-label"))!)
    .split(", ")
    .at(-1)!;

  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
  await expect(dialog).toBeVisible();
  // Presets load from the daemon. The digit shortcut is broken (see the
  // fixme below), so pick the first preset directly.
  await dialog.getByRole("listitem").first().click();

  await expect(dialog).toHaveCount(0);
  await expect(rowById(page, rowId)).toHaveCount(0);
  await expect(page.getByText(/^Snoozed \d+ messages?$/)).toBeVisible();

  await pressSequence(page, "g", "n");
  await expect(page).toHaveURL(/\/snoozed$/);
  const wake = page.getByRole("button", { name: `Wake ${subject} now` }).first();
  await expect(wake).toBeVisible();

  // Keyboard path: the hover toolbar covers the button for pointers (fixme below).
  await wake.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByText("Back in your inbox")).toBeVisible();
  await expect(page.getByRole("button", { name: `Wake ${subject} now` })).toHaveCount(0);

  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(
    mailList(page).getByRole("option", { name: new RegExp(escapeRegExp(subject)) }).first(),
  ).toBeVisible();
});

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

// BUG: the snooze dialog opens with focus in the "Or type a time" input, and
// the preset digits are ignored while an input has focus, so Z then 1 types
// "1" into the field instead of choosing the first preset its key chip shows.
test.fixme("Z then 1 chooses the first preset", async ({ page }) => {
  await openList(page, "/m/inbox");
  const rowId = await cursorRowId(page);
  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
  await expect(dialog.getByRole("listitem").first()).toBeVisible();
  await page.keyboard.press("1");
  await expect(dialog).toHaveCount(0);
  await expect(rowById(page, rowId)).toHaveCount(0);
});

// BUG: on /snoozed the row's hover quick-action toolbar (archive, trash,
// snooze, star) is drawn over the trailing "Wake now" button, so a pointer
// can never click it: hovering the row to reach the button raises the toolbar
// on top of it.
test.fixme("Wake now is clickable with the mouse", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
  await dialog.getByRole("listitem").first().click();
  await pressSequence(page, "g", "n");
  const wake = page.getByRole("button", { name: /^Wake .* now$/ }).first();
  await wake.click({ timeout: 5_000 });
  await expect(page.getByText("Back in your inbox")).toBeVisible();
});

// BUG: snoozing a multi-message conversation lists each of its messages as a
// separate row on /snoozed, and "Wake now" wakes only that one message, so the
// conversation is back in Inbox while its other messages stay snoozed.
test.fixme("waking a snoozed conversation wakes all of its messages", async ({ page }) => {
  await openList(page, "/m/inbox");
  const row = mailList(page).getByRole("option", { name: /5 messages in conversation/ }).first();
  await row.click();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Z");
  await page.getByRole("dialog", { name: "Snooze until…" }).getByRole("listitem").first().click();
  await pressSequence(page, "g", "n");
  const wake = page.getByRole("button", { name: /^Wake .* now$/ });
  await expect(wake).toHaveCount(1);
});
