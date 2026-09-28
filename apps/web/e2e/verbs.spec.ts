import { expect, test, type Page } from "@playwright/test";

import { VERB_FEEDBACK, type Verb } from "../src/features/mail-actions/verbFeedback";
import { cursorRowId, mailList, openList, rowById } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

/**
 * Every verb the keyboard runs on a row, with the way it is undone. The
 * toast's words come from the verb table, so the table and the app can't
 * disagree. Dialog verbs (labels, move) and place verbs (sweep, pin,
 * move sender) have their own journeys in labels.spec and places.spec.
 */
const ROW_VERBS: { verb: Verb; keys: string[]; leaves: boolean }[] = [
  { verb: "archive", keys: ["e"], leaves: true },
  { verb: "read-and-archive", keys: ["m"], leaves: true },
  { verb: "trash", keys: ["#"], leaves: true },
  { verb: "spam", keys: ["!"], leaves: true },
  { verb: "star", keys: ["s"], leaves: false },
  { verb: "unread", keys: ["U"], leaves: false },
  { verb: "snooze", keys: ["Z", "1"], leaves: true },
];

function lastToast(page: Page, text: string) {
  return page.locator("[data-sonner-toast]").filter({ hasText: text }).last();
}

test("each row verb says what it did in the table's words, and u undoes it", async ({ page }) => {
  await openList(page, "/m/inbox");
  await mailList(page).focus();
  for (const { verb, keys, leaves } of ROW_VERBS) {
    const entry = VERB_FEEDBACK[verb];
    expect(["daemon-mutation", "wake"]).toContain(entry.undo);
    // Snooze acts on messages; a conversation row with other messages
    // stays in the inbox, so it takes a single-message row (as snooze.spec).
    if (verb === "snooze") await cursorToSingleMessageRow(page);
    const rowId = await cursorRowId(page);
    for (const key of keys) {
      await page.keyboard.press(key);
      // A dialog verb (snooze) takes its next key once the dialog is up.
      if (key === "Z")
        await expect(page.getByRole("dialog", { name: "Snooze until…" })).toBeVisible();
    }
    await expect(lastToast(page, entry.pastTense), verb).toBeVisible();
    if (leaves) await expect(rowById(page, rowId), verb).toHaveCount(0);
    await page.keyboard.press("u");
    await expect(rowById(page, rowId), verb).toBeVisible();
    await expect(page.getByText(/^Undone$/).first(), verb).toBeVisible();
  }
});

test("reply later adds to the queue in the table's words, and the toast undoes it", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  await mailList(page).focus();
  await page.keyboard.press("b");
  const toast = lastToast(page, VERB_FEEDBACK["reply-later"].pastTense);
  await expect(toast).toBeVisible();
  const queued = page.waitForResponse((response) => response.url().includes("reply-later"));
  await toast.getByRole("button", { name: "Undo" }).click();
  expect((await queued).ok()).toBe(true);
});

async function cursorToSingleMessageRow(page: Page): Promise<void> {
  for (let step = 0; step < 15; step += 1) {
    const name = (await rowById(page, await cursorRowId(page)).getAttribute("aria-label")) ?? "";
    if (!/messages in conversation/.test(name)) return;
    await page.keyboard.press("j");
  }
  throw new Error("no single-message conversation in the first rows");
}
