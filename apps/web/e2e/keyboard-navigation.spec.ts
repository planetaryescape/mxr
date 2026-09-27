import { expect, test } from "@playwright/test";

import { expectCursorOn, mailList, mailRows, openList, reader, renderedRowIds } from "./helpers/mail";
import { openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test("h hands the keyboard to the sidebar; j and Enter open the next mailbox", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("h");
  await expect(mailList(page)).not.toHaveAttribute("data-active-pane", "true");
  await page.keyboard.press("j");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/m\/starred$/);
});

test("collapsed sidebar keeps the expand control reachable", async ({ page }) => {
  await openApp(page, "/m/inbox");

  await page.getByRole("button", { name: "Collapse sidebar" }).click();
  const expand = page.getByRole("button", { name: "Expand sidebar" });
  await expect(expand).toBeVisible();

  const rects = await expand.evaluate((button) => {
    const sidebar = button.closest(".app-shell-sidebar");
    if (!sidebar) throw new Error("sidebar shell not found");
    const buttonRect = button.getBoundingClientRect();
    const sidebarRect = sidebar.getBoundingClientRect();
    return {
      buttonLeft: buttonRect.left,
      buttonRight: buttonRect.right,
      sidebarLeft: sidebarRect.left,
      sidebarRight: sidebarRect.right,
    };
  });
  expect(rects.buttonLeft).toBeGreaterThanOrEqual(rects.sidebarLeft);
  expect(rects.buttonRight).toBeLessThanOrEqual(rects.sidebarRight);

  await expand.click();
  await expect(page.getByRole("button", { name: "Collapse sidebar" })).toBeVisible();
});

test("the open reader owns j/k until h returns the keyboard to the list", async ({ page }) => {
  await openList(page, "/m/inbox");
  const [first, second] = await renderedRowIds(page);

  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  const openedThreadUrl = page.url();
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");

  // In the reader j scrolls; it does not change the conversation.
  await page.keyboard.press("j");
  await expect(page).toHaveURL(openedThreadUrl);
  await expectCursorOn(page, first!);

  // Back in the list, j moves the cursor and previews the next conversation.
  await page.keyboard.press("h");
  await expect(mailList(page)).toHaveAttribute("data-active-pane", "true");
  await page.keyboard.press("j");
  await expectCursorOn(page, second!);
  await expect(page).not.toHaveURL(openedThreadUrl);
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
});

test("reader views switch with R and H", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  const views = reader(page).getByRole("radiogroup", { name: "Message view" });
  await expect(views.getByRole("radio", { name: "Formatted" })).toBeChecked();

  await page.keyboard.press("R");
  await expect(views.getByRole("radio", { name: "Reader" })).toBeChecked();
  await page.keyboard.press("H");
  await expect(views.getByRole("radio", { name: "Formatted" })).toBeChecked();
});

test("number keys jump between the TUI tabs", async ({ page }) => {
  await openList(page, "/m/inbox");

  const tabs: [string, RegExp][] = [
    ["2", /\/search$/],
    ["3", /\/rules$/],
    ["4", /\/accounts$/],
    ["5", /\/diagnostics$/],
    ["6", /\/analytics(\/storage)?$/],
    ["7", /\/deliveries$/],
    ["1", /\/m\/inbox$/],
  ];
  for (const [key, url] of tabs) {
    // The search page focuses its query box when empty; leave it first.
    if (new URL(page.url()).pathname === "/search") await page.keyboard.press("Escape");
    await page.keyboard.press(key);
    await expect(page, `digit ${key}`).toHaveURL(url);
  }
});

test("g chords jump to the TUI's views", async ({ page }) => {
  await openList(page, "/m/inbox");
  const chords: [string, RegExp][] = [
    ["s", /\/m\/starred$/],
    ["t", /\/m\/sent$/],
    ["a", /\/m\/archive$/],
    ["d", /\/drafts$/],
    ["n", /\/snoozed$/],
    ["q", /\/reply-queue$/],
    ["o", /\/owed$/],
    ["A", /\/analytics/],
    ["i", /\/m\/inbox$/],
  ];
  for (const [key, url] of chords) {
    await page.keyboard.press("g");
    await page.keyboard.press(key);
    await expect(page, `g ${key}`).toHaveURL(url);
  }
});

test("analytics opens dashboards through TUI-style tabs", async ({ page }) => {
  await openApp(page, "/analytics");
  await expect(page).toHaveURL(/\/analytics\/storage$/);

  const tabs = page.getByRole("tablist", { name: "Analytics dashboards" });
  await expect(tabs).toBeVisible();
  await expect(tabs.getByRole("tab", { name: "Storage" })).toHaveAttribute("aria-selected", "true");

  await tabs.getByRole("tab", { name: "Wrapped" }).click();
  await expect(page).toHaveURL(/\/analytics\/wrapped$/);
  await expect(tabs.getByRole("tab", { name: "Wrapped" })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("heading", { level: 1, name: "Wrapped" })).toBeVisible();
});

test("list rows and reader text are readable by default", async ({ page }) => {
  // Largest text in a subtree: the subject in a row, the body in a message.
  const largestText = (root: Element) =>
    Math.max(
      ...[...root.querySelectorAll("span, p, pre, div")]
        .filter((el) => [...el.childNodes].some((n) => n.nodeType === 3 && (n.textContent ?? "").trim()))
        .map((el) => Number.parseFloat(getComputedStyle(el).fontSize)),
    );

  await openList(page, "/m/inbox");
  expect(await mailRows(page).first().evaluate(largestText)).toBeGreaterThanOrEqual(13);

  await page.keyboard.press("Enter");
  await expect(reader(page)).toHaveAttribute("data-active-pane", "true");
  await page.keyboard.press("R");
  await expect(reader(page).getByRole("radio", { name: "Reader" })).toBeChecked();
  expect(
    await reader(page).getByTestId("thread-message").last().evaluate(largestText),
  ).toBeGreaterThanOrEqual(14);
});

test("the reader fills the reading pane at 1920px", async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 900 });
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  const message = reader(page).getByTestId("thread-message").last();
  await expect(message).toBeVisible();
  const widths = await reader(page).evaluate((node) => {
    const card = node.querySelector('[data-testid="thread-message"]:last-of-type');
    return {
      reader: node.getBoundingClientRect().width,
      message: card?.getBoundingClientRect().width ?? 0,
    };
  });
  // Messages cap at a readable measure but still use most of the pane.
  expect(widths.message).toBeGreaterThan(Math.min(widths.reader * 0.8, 900));
});
