import { expect, test, type Page } from "@playwright/test";

import { expectCursorOn, mailRows, rowById } from "./helpers/mail";
import { openApp, readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

const auth = () => ({ Authorization: `Bearer ${readE2EState().token}` });

interface DeskRowAnswer {
  thread_id: string;
  message_id: string;
  message_ids: string[];
  unread?: boolean;
}

type Lane = "owed" | "waiting";

async function laneRows(page: Page, lane: Lane): Promise<DeskRowAnswer[]> {
  const response = await page.request.get(
    `${readE2EState().bridgeUrl}/api/v1/mail/desk?lane_limit=100`,
    { headers: auth() },
  );
  const desk = (await response.json()) as Record<Lane, { rows: DeskRowAnswer[] }>;
  return desk[lane].rows;
}

/** The list row's DOM id: thread rows are keyed by thread, single messages by id. */
function domId(row: DeskRowAnswer): string {
  return row.message_ids.length > 1
    ? `mail-row-thread-${row.thread_id}`
    : `mail-row-${row.message_id}`;
}

interface MessageState {
  id: string;
  unread: boolean;
  inInbox: boolean;
}

/** Each message's read state and inbox membership, from the daemon. */
async function threadState(page: Page, threadId: string): Promise<MessageState[]> {
  const response = await page.request.get(
    `${readE2EState().bridgeUrl}/api/v1/mail/threads/${encodeURIComponent(threadId)}`,
    { headers: auth() },
  );
  expect(response.ok()).toBe(true);
  const thread = (await response.json()) as {
    messages: { id: string; unread: boolean; labels?: { name: string }[] }[];
  };
  return thread.messages.map((message) => ({
    id: message.id,
    unread: message.unread,
    inInbox: (message.labels ?? []).some((label) => label.name.toLowerCase() === "inbox"),
  }));
}

test("e on an owed row is Done: it leaves at once, is read and archived, and u puts it back as it was", async ({
  page,
}) => {
  const rows = await laneRows(page, "owed");
  const row = rows[0]!;
  const before = await threadState(page, row.thread_id);
  expect(before.some((message) => message.inInbox)).toBe(true);

  await openApp(page, "/desk?lane=owed");
  await expect(mailRows(page).first()).toBeVisible();
  await expectCursorOn(page, domId(row));

  // Hold the request so what the user sees is the optimistic removal.
  let release: () => void = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/api/v1/mail/desk/done", async (route) => {
    await held;
    await route.continue();
  });
  const done = page.waitForResponse("**/api/v1/mail/desk/done");
  await page.keyboard.press("e");
  await expect(rowById(page, domId(row))).toHaveCount(0);
  const refetched = page.waitForResponse((r) => r.url().includes("/api/v1/mail/desk?"));
  release();
  const response = await done;
  expect(response.ok()).toBe(true);
  const body = (await response.json()) as { mutation_id?: string; items: { lane: string }[] };
  expect(body.items[0]?.lane).toBe("owed");
  expect(body.mutation_id).toBeTruthy();
  await expect(page.getByText(/^Done$/)).toBeVisible();

  const after = await threadState(page, row.thread_id);
  expect(after.every((message) => !message.unread && !message.inInbox)).toBe(true);
  // The refetched desk agrees.
  await refetched;
  await expect(rowById(page, domId(row))).toHaveCount(0);

  const undone = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await page.keyboard.press("u");
  expect((await undone).ok()).toBe(true);
  await expect(rowById(page, domId(row))).toBeVisible();
  expect(await threadState(page, row.thread_id)).toEqual(before);
});

test("the row's check is Done too, and the toast's Undo brings it back", async ({ page }) => {
  // Not the cursor's row: the check shows there without a pointer.
  const row = (await laneRows(page, "waiting"))[1]!;
  const before = await threadState(page, row.thread_id);

  await openApp(page, "/desk?lane=waiting");
  const target = rowById(page, domId(row));
  await expect(target).toBeVisible();
  const check = target.getByTestId("desk-done");
  // With a mouse it waits for the pointer.
  await expect(check).toHaveCSS("opacity", "0");
  await target.hover();
  await expect(check).toHaveCSS("opacity", "1");

  const done = page.waitForResponse("**/api/v1/mail/desk/done");
  await check.click();
  await expect(target).toHaveCount(0);
  const body = (await (await done).json()) as { items: { lane: string; dismissed: boolean }[] };
  expect(body.items[0]).toMatchObject({ lane: "waiting", dismissed: true });
  // Waiting on is marked read but never archived.
  const after = await threadState(page, row.thread_id);
  expect(after.every((message) => !message.unread)).toBe(true);
  expect(after.map((message) => message.inInbox)).toEqual(before.map((m) => m.inInbox));
  // Opening the row was not part of it.
  await expect(page).toHaveURL(/\/desk\?lane=waiting$/);

  const undone = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await page.getByRole("button", { name: "Undo" }).click();
  expect((await undone).ok()).toBe(true);
  await expect(rowById(page, domId(row))).toBeVisible();
  expect(await threadState(page, row.thread_id)).toEqual(before);
});

test("a held e puts away one row, not the lane", async ({ page }) => {
  await openApp(page, "/desk?lane=owed");
  await expect(mailRows(page).first()).toBeVisible();
  const requests: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("/api/v1/mail/desk/done")) requests.push(request.url());
  });
  const done = page.waitForResponse("**/api/v1/mail/desk/done");
  await page.keyboard.down("e");
  // Auto-repeat, as a held key sends it.
  for (let i = 0; i < 5; i += 1) {
    await page.evaluate(() =>
      (document.activeElement ?? document.body).dispatchEvent(
        new KeyboardEvent("keydown", { key: "e", repeat: true, bubbles: true, cancelable: true }),
      ),
    );
  }
  await page.keyboard.up("e");
  const body = (await (await done).json()) as { mutation_id: string };
  await page.waitForTimeout(500);
  expect(requests).toHaveLength(1);
  await page.request.post(`${readE2EState().bridgeUrl}/api/v1/mail/mutations/undo`, {
    headers: auth(),
    data: { mutation_id: body.mutation_id },
  });
});
