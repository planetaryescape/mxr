import { expect, test } from "@playwright/test";

import { openApp } from "./helpers/state";

test("keyboard-only mailbox navigation opens the focused thread", async ({ page }) => {
  await openApp(page, "/m/inbox");

  const list = page.getByTestId("mailbox-list");
  await expect(list).toBeVisible();
  await expect(list.getByRole("article").first()).toBeVisible();

  await page.keyboard.press("j");
  await page.keyboard.press("Enter");

  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  await expect(page.getByRole("radio", { name: /^reader$/i })).toBeVisible();
  await expect(page.getByRole("radio", { name: /^HTML$/ })).toBeVisible();
});

test("h moves mailbox focus to sidebar navigation", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByTestId("mailbox-list")).toBeVisible();

  await page.keyboard.press("h");
  await page.keyboard.press("j");

  await expect(page.getByRole("link", { name: /^Search/ })).toHaveAttribute("data-focused", "true");
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

test("opened reader owns j/k until focus returns to mailbox list", async ({ page }) => {
  await openApp(page, "/m/inbox");
  const list = page.getByTestId("mailbox-list");
  await expect(list).toBeVisible();

  await page.keyboard.press("o");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  const openedThreadUrl = page.url();
  await expect(page.getByRole("article", { name: "Thread reader" })).toHaveAttribute(
    "data-active-pane",
    "true",
  );

  await page.keyboard.press("j");
  await expect(page).toHaveURL(openedThreadUrl);

  await page.keyboard.press("h");
  await page.keyboard.press("j");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
  expect(page.url()).not.toBe(openedThreadUrl);
});

test("returning from reader restores mailbox j/k navigation", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByTestId("mailbox-list")).toBeVisible();

  await page.keyboard.press("l");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);

  await page.keyboard.press("u");
  await expect(page).toHaveURL(/\/m\/inbox$/);

  await page.keyboard.press("j");
  await page.keyboard.press("o");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
});

test("number keys jump between primary workspaces", async ({ page }) => {
  await openApp(page, "/m/inbox");
  await expect(page.getByTestId("mailbox-list")).toBeVisible();

  await page.keyboard.press("Digit2");
  await expect(page).toHaveURL(/\/search$/);

  await page.keyboard.press("Digit3");
  await expect(page).toHaveURL(/\/analytics\/storage$/);

  await page.keyboard.press("Digit4");
  await expect(page).toHaveURL(/\/rules$/);

  await page.keyboard.press("Digit5");
  await expect(page).toHaveURL(/\/screener$/);

  await page.keyboard.press("Digit1");
  await expect(page).toHaveURL(/\/m\/inbox$/);
});

test("compact density materially tightens mailbox rows", async ({ page }) => {
  await openApp(page, "/m/inbox");
  const firstRow = page.getByTestId("mailbox-list").getByRole("article").first();
  await expect(firstRow).toBeVisible();

  await page.getByRole("button", { name: /Regular/ }).click();
  const regularHeight = await firstRow.evaluate((node) => node.getBoundingClientRect().height);

  await page.getByRole("button", { name: /Compact/ }).click();
  const compactHeight = await firstRow.evaluate((node) => node.getBoundingClientRect().height);

  expect(compactHeight).toBeLessThan(regularHeight - 10);
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

test("mailbox and thread reader text are readable by default", async ({ page }) => {
  await openApp(page, "/m/inbox");

  const list = page.getByTestId("mailbox-list");
  await expect(list).toBeVisible();

  const subjectSize = await list
    .getByRole("article")
    .first()
    .getByRole("heading", { level: 2 })
    .evaluate((node) => Number.parseFloat(window.getComputedStyle(node).fontSize));
  expect(subjectSize).toBeGreaterThanOrEqual(13);

  await list.getByRole("article").first().press("Enter");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);

  const bodySize = await page
    .locator("pre")
    .first()
    .evaluate((node) => Number.parseFloat(window.getComputedStyle(node).fontSize));
  expect(bodySize).toBeGreaterThanOrEqual(15);
});

test("thread reader uses the available reading pane width", async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 900 });
  await openApp(page, "/m/inbox");

  const list = page.getByTestId("mailbox-list");
  await expect(list).toBeVisible();
  await list.getByRole("article").first().press("Enter");
  await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);

  const reader = page.locator("article").last();
  const message = page.locator("pre").first().locator("xpath=ancestor::section[1]");
  await expect(message).toBeVisible();

  const widths = await reader.evaluate((readerNode) => {
    const messageNode = readerNode.querySelector("section");
    if (!messageNode) throw new Error("message section not found");
    return {
      reader: readerNode.getBoundingClientRect().width,
      message: messageNode.getBoundingClientRect().width,
    };
  });
  expect(widths.message).toBeGreaterThan(widths.reader * 0.82);
});
