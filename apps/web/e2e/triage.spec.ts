import { expect, test } from "@playwright/test";

import {
  cursorRowId,
  expectCursorOn,
  mailList,
  openList,
  pressSequence,
  reader,
  renderedRowIds,
  rowById,
  stableRowName,
} from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

test("keyboard-only triage: open, archive-and-advance, find it in All Mail, undo", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const [first, second] = await renderedRowIds(page);
  expect(first && second).toBeTruthy();
  await expectCursorOn(page, first!);

  await page.keyboard.press("j");
  await expectCursorOn(page, second!);
  await page.keyboard.press("k");
  await expectCursorOn(page, first!);

  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  const openedUrl = page.url();
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");
  const archivedName = stableRowName((await rowById(page, first!).getAttribute("aria-label"))!);

  await page.keyboard.press("e");
  // Archive-and-advance: the reader moves on and the row leaves Inbox at once.
  await expect(page).not.toHaveURL(openedUrl);
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  await expect(rowById(page, first!)).toHaveCount(0);
  await expect(page.getByText(/^Archived \d+ messages?$/)).toBeVisible();

  await pressSequence(page, "g", "a");
  await expect(page).toHaveURL(/\/m\/archive$/);
  // All Mail titles a conversation by its latest message, which may be a "Re:".
  await expect(
    mailList(page).getByRole("option", { name: conversationPattern(archivedName) }).first(),
  ).toBeVisible();

  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(rowById(page, first!)).toHaveCount(0);

  await page.keyboard.press("u");
  await expect(page.getByText(/^Undone$/)).toBeVisible();
  await expect(rowById(page, first!)).toBeVisible();
});

test("select three with x, archive them together, undo brings all three back", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  const ids = (await renderedRowIds(page)).slice(0, 4);
  await expectCursorOn(page, ids[0]!);

  await pressSequence(page, "x", "x", "x");
  for (const id of ids.slice(0, 3)) {
    await expect(rowById(page, id)).toHaveAttribute("aria-selected", "true");
  }
  await expect(rowById(page, ids[3]!)).toHaveAttribute("aria-selected", "false");
  // x advances the cursor after each toggle.
  expect(await cursorRowId(page)).toBe(ids[3]);

  await page.keyboard.press("e");
  for (const id of ids.slice(0, 3)) await expect(rowById(page, id)).toHaveCount(0);
  await expect(rowById(page, ids[3]!)).toBeVisible();
  // The rows leave at once; the toast waits for the daemon's answer, which
  // can take longer than the default wait while the full suite loads it.
  await expect(page.getByText(/^Archived \d+ messages$/)).toBeVisible({ timeout: 15_000 });
  await expect(mailList(page)).toHaveAttribute("aria-activedescendant", ids[3]!);

  await page.keyboard.press("u");
  for (const id of ids.slice(0, 3)) await expect(rowById(page, id)).toBeVisible();
});

function conversationPattern(stableName: string): RegExp {
  const parts = stableName.split(", ");
  const subject = parts.pop()!.replace(/^Re: /, "");
  const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`${escape(parts.join(", "))}, (Re: )?${escape(subject)},`);
}
