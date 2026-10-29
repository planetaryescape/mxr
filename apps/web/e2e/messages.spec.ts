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
