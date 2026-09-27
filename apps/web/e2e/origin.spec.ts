import { expect, test } from "@playwright/test";

import { expectCursorOn, mailRows, openList, reader, renderedRowIds } from "./helpers/mail";
import { openApp } from "./helpers/state";

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
  await openList(page, "/search?q=canary");
  const ids = await renderedRowIds(page);
  expect(ids.length).toBeGreaterThanOrEqual(3);

  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");
  const opened = page.url();

  await page.keyboard.press("]");
  await expect(page).not.toHaveURL(opened);
  await expect(page).toHaveURL(/\/search\/[^/?]+\?q=canary/);
  await expect(page.getByText(/^Archived \d+ messages?$/)).toBeVisible();
  // Archived mail still matches the query, so the next result is the one open.
  await expectCursorOn(page, ids[2]!);

  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(/\/search\?q=canary$/);
  await expectCursorOn(page, ids[2]!);

  await page.keyboard.press("u");
  await expect(page.getByText(/^Undone$/)).toBeVisible();
});

test("a hard refresh on /search?q= keeps the query and results", async ({ page }) => {
  await openApp(page, "/search?q=canary");
  await expect(mailRows(page).first()).toBeVisible();
  await page.reload();
  await expect(page).toHaveURL(/\/search\?q=canary/);
  await expect(page.getByLabel("Search query")).toHaveValue("canary");
  await expect(mailRows(page).first()).toBeVisible();
});
