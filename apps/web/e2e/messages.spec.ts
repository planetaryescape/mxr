import { expect, test, type Page } from "@playwright/test";

import { archiveViaBridge } from "./helpers/archive";
import { freshReplyRequest } from "./helpers/reply-request";
import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface Row {
  topics: { thread_id: string }[];
}

interface MessagesAnswer {
  messages: { your_turn: Row[]; pinned: Row[]; recent: Row[]; quiet: Row[] };
}

/** Whether the daemon's attention view (Messages' default) still holds `thread`. */
async function needsAttention(page: Page, thread: string): Promise<boolean> {
  const { messages } = await bridge<MessagesAnswer>(page, "/api/v1/mail/people?limit=500");
  return [...messages.your_turn, ...messages.pinned, ...messages.recent, ...messages.quiet].some(
    (row) => row.topics.some((topic) => topic.thread_id === thread),
  );
}

test("done in Messages takes the conversation out of attention, and u brings it back", async ({
  page,
}) => {
  const { person, thread, messageIds } = await freshReplyRequest(page);
  try {
    await openApp(page, `/messages?person=${encodeURIComponent(person)}&topic=${thread}`);
    await expect(page.getByTestId("conversation")).toBeVisible();
    expect(await needsAttention(page, thread)).toBe(true);

    await page.keyboard.press("e");
    await expect.poll(() => needsAttention(page, thread), { timeout: 15_000 }).toBe(false);

    // The toast's words depend on what was left open ("Undone", "Restored: …");
    // the daemon's answer is the check.
    await page.keyboard.press("u");
    await expect.poll(() => needsAttention(page, thread), { timeout: 15_000 }).toBe(true);
  } finally {
    // Specs share one mailbox: leave no new turn behind for the others.
    await archiveViaBridge(page, messageIds);
  }
});

test("on a phone, Back from a reply saves it as a Draft on the person's row, and opening them resumes it", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { person, thread, messageIds } = await freshReplyRequest(page);
  const words = `Thursday at ten works ${Date.now().toString(36)}`;
  const reply = page.locator(`#inline-composer-slot-${thread} [data-compose-surface]`);
  const row = page.locator(`[data-testid="messages-row"][data-row-id="${person}"]`);
  try {
    await openApp(page, `/messages?person=${encodeURIComponent(person)}&topic=${thread}`);
    await expect(page.getByTestId("conversation")).toBeVisible();
    await page.getByTestId("composer").click();
    const body = reply.locator(".cm-content");
    await expect(body).toBeVisible();
    await body.click();
    await page.keyboard.press("i");
    await page.keyboard.type(words);

    await page.getByRole("button", { name: "Back to the people" }).click();
    // The people, not a sheet over them: the reply closed, saved.
    await expect(page.getByRole("dialog")).toHaveCount(0);
    const preview = row.getByTestId("row-preview");
    await expect(preview).toHaveAttribute("data-kind", "draft", { timeout: 15_000 });
    await expect(preview).toHaveText(`Draft: ${words}`);

    await row.click();
    await expect(reply.locator(".cm-content")).toContainText(words);
    await expect(reply.locator(".cm-content")).toBeFocused();
  } finally {
    await archiveViaBridge(page, messageIds);
  }
});
