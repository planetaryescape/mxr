import { expect, test } from "@playwright/test";

import {
  cursorRowId,
  expectCursorOn,
  mailList,
  mailRows,
  pressSequence,
  reader,
  renderedRowIds,
  rowById,
} from "./helpers/mail";
import { openApp, readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

function sidebar(page: import("@playwright/test").Page) {
  return page.getByRole("complementary", { name: "Mailboxes" });
}

test("the app opens on the desk: lanes with reasons, and work-only badges", async ({ page }) => {
  await openApp(page);
  await expect(page).toHaveURL(/\/desk$/);
  const heading = page.getByRole("heading", { level: 1 });
  await expect(heading).toContainText(/(morning|afternoon|evening|night)\./);
  await expect(heading.getByRole("link", { name: /\d+ repl(y|ies)/ })).toBeVisible();
  for (const lane of ["You owe", "Waiting on", "New from people"]) {
    await expect(mailList(page).getByText(lane, { exact: true })).toBeVisible();
  }
  // Every row says why it is there.
  await expect(mailRows(page).first()).toHaveAttribute(
    "aria-label",
    /replied to your message|wrote to you|messages since you last wrote|copied you/,
  );
  await expect(page.getByRole("navigation", { name: "Everything else" })).toBeVisible();

  // The desk's badge is owed plus due; the inbox carries no unread count.
  const state = readE2EState();
  const desk = (await (
    await page.request.get(`${state.bridgeUrl}/api/v1/mail/desk`, {
      headers: { authorization: `Bearer ${state.token}` },
    })
  ).json()) as { owed: { total: number }; due: { total: number } };
  await expect(sidebar(page).getByRole("link", { name: "Desk" })).toContainText(
    String(desk.owed.total + desk.due.total),
  );
  await expect(sidebar(page).getByRole("link", { name: "Inbox" })).not.toContainText(/\d/);
});

test("desk journey: j, Done optimistically, undo, open and come back to the same row", async ({
  page,
}) => {
  await openApp(page);
  await expect(mailRows(page).first()).toBeVisible();
  const [first, second, third] = await renderedRowIds(page);
  expect(first && second && third).toBeTruthy();
  await expectCursorOn(page, first!);

  await page.keyboard.press("j");
  await expectCursorOn(page, second!);

  // Hold Done so what the user sees is the optimistic removal.
  let release: () => void = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/api/v1/mail/desk/done", async (route) => {
    await held;
    await route.continue();
  });
  const done = page.waitForResponse("**/api/v1/mail/desk/done");
  await page.keyboard.press("e");
  await expect(rowById(page, second!)).toHaveCount(0);
  // The cursor stays in place, so the next row comes up under it.
  await expectCursorOn(page, third!);
  release();
  expect((await done).ok()).toBe(true);
  await page.unroute("**/api/v1/mail/desk/done");
  // The refetched desk agrees: a conversation put away is off the desk.
  await expect(rowById(page, second!)).toHaveCount(0);

  const undone = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await page.keyboard.press("u");
  expect((await undone).ok()).toBe(true);
  await expect(rowById(page, second!)).toBeVisible();

  // Enter opens the thread; Esc comes back to the desk on the same row.
  const cursor = await cursorRowId(page);
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/desk\/[^/]+$/);
  await expect(reader(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(/\/desk$/);
  await expect(reader(page)).toHaveCount(0);
  await expectCursorOn(page, cursor);
});

test("g i goes to arrival order and g h comes back; Waiting on is one lane in full", async ({
  page,
}) => {
  await openApp(page);
  await expect(mailRows(page).first()).toBeVisible();
  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(page.getByRole("heading", { level: 1, name: "Inbox" })).toBeVisible();
  await pressSequence(page, "g", "h");
  await expect(page).toHaveURL(/\/desk$/);

  await sidebar(page).getByRole("link", { name: "Waiting on" }).click();
  await expect(page).toHaveURL(/\/desk\?lane=waiting$/);
  await expect(page.getByRole("heading", { level: 1, name: "Waiting on" })).toBeVisible();
  await expect(mailList(page).getByText("You owe", { exact: true })).toHaveCount(0);
  await expect(mailRows(page).first()).toBeVisible();
});

test("a person who prefers arrival order can make the inbox their home", async ({ page }) => {
  await openApp(page, "/settings/theme");
  await page.getByRole("combobox", { name: "Home" }).click();
  await page.getByRole("option", { name: "Inbox" }).click();
  await openApp(page, "/");
  await expect(page).toHaveURL(/\/m\/inbox$/);
});
