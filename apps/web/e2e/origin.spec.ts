import { randomUUID } from "node:crypto";

import { expect, test } from "@playwright/test";

import { deliver } from "./helpers/spool";
import { expectCursorOn, mailRows, openList, reader, renderedRowIds } from "./helpers/mail";
import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

function pathAndQuery(url: string): string {
  const parsed = new URL(url);
  return `${parsed.pathname}${parsed.search}`;
}

test("Esc from a conversation opened in a label lens returns to that label", async ({ page }) => {
  await openList(page, "/m/label/work");
  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/m\/label\/work\/[^/?]+$/);
  await expect(reader(page)).toBeVisible();

  await page.keyboard.press("Escape");
  expect(pathAndQuery(page.url())).toBe("/m/label/work");
  await expect(page.getByRole("heading", { level: 1, name: "Work" })).toBeVisible();
});

test("Esc from a search result returns to the same query with every parameter", async ({
  page,
}) => {
  const origin = "/search?q=canary&sort=newest";
  await openList(page, origin);
  const originUrl = pathAndQuery(page.url());
  expect(originUrl).toContain("q=canary");
  expect(originUrl).toContain("sort=newest");
  const [, second] = await renderedRowIds(page);

  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/search\/[^/?]+\?/);
  // The open conversation keeps the query in its URL.
  expect(new URL(page.url()).search).toBe(new URL(originUrl, page.url()).search);
  await expect(reader(page)).toBeVisible();

  await page.keyboard.press("Escape");
  expect(pathAndQuery(page.url())).toBe(originUrl);
  await expect(page.getByLabel("Search query")).toHaveValue("canary");
  await expectCursorOn(page, second!);
});

test("archive-and-advance from search, then Esc lands on the next result", async ({ page }) => {
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

test("a hard refresh on /search?q= keeps the query and results", async ({ page }) => {
  await openApp(page, "/search?q=canary");
  await expect(mailRows(page).first()).toBeVisible();
  await page.reload();
  await expect(page).toHaveURL(/\/search\?q=canary/);
  await expect(page.getByLabel("Search query")).toHaveValue("canary");
  await expect(mailRows(page).first()).toBeVisible();
});
