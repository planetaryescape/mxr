import { randomUUID } from "node:crypto";

import { expect, test } from "@playwright/test";

import { expectCursorOn, mailRows, reader, renderedRowIds } from "./helpers/mail";
import { deliver } from "./helpers/spool";
import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test("new mail is searchable; archive-and-advance from results, Esc back to the query, undo", async ({
  page,
}) => {
  const query = randomUUID().slice(0, 8);
  for (const index of [0, 1, 2]) {
    deliver({
      from: `archive-${index}@example.test`,
      subject: `${query} inbox fixture ${index}`,
      body: `Searchable Inbox fixture ${query} ${index}.`,
    });
  }
  await bridge(page, "/api/v1/mail/sync", {});
  const origin = `/search?q=${query}`;
  await openApp(page, origin);
  await expect(mailRows(page)).toHaveCount(3, { timeout: 15_000 });
  const ids = await renderedRowIds(page);
  expect(ids).toHaveLength(3);

  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");
  const opened = page.url();
  const archivedThreadId = decodeURIComponent(new URL(opened).pathname.split("/").at(-1) ?? "");
  if (!archivedThreadId) throw new Error("opened search result has no thread id");
  const threadHasInboxMessage = async () => {
    const { messages } = await bridge<{
      messages: { labels?: { name: string }[] }[];
    }>(page, `/api/v1/mail/threads/${encodeURIComponent(archivedThreadId)}`);
    return messages.some((message) =>
      (message.labels ?? []).some((label) => label.name.toLowerCase() === "inbox"),
    );
  };
  expect(await threadHasInboxMessage()).toBe(true);

  await page.keyboard.press("]");
  await expect(page).not.toHaveURL(opened);
  await expect(page).toHaveURL(new RegExp(`/search/[^/?]+\\?q=${query}$`));
  await expect(page.getByText(/^Archived \d+ messages?$/)).toBeVisible();
  await expect.poll(threadHasInboxMessage).toBe(false);
  // Archived mail still matches the query, so the next result is the one open.
  await expectCursorOn(page, ids[2]!);

  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(new RegExp(`/search\\?q=${query}$`));
  await expectCursorOn(page, ids[2]!);

  const undoResponse = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await page.keyboard.press("u");
  expect((await undoResponse).ok()).toBe(true);
  await expect(page.getByText(/^Undone$/)).toBeVisible();
  await expect.poll(threadHasInboxMessage).toBe(true);
});
