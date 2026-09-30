import { expect, test, type Page } from "@playwright/test";

import { expectCursorOn, mailRows, rowById } from "./helpers/mail";
import { openApp, readE2EState, restartDaemon } from "./helpers/state";

/**
 * Timed deferral from one key: `b` asks when, the exact time shows before
 * anything is stored, the toast names it, and the conversation comes back
 * on its own at that time, across a daemon restart.
 */

test.use({ viewport: { width: 1440, height: 900 } });

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

const auth = () => ({ Authorization: `Bearer ${readE2EState().token}` });

interface DeskRowAnswer {
  thread_id: string;
  message_id: string;
  message_ids: string[];
  reason: string;
  back_at?: string | null;
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

function lastToast(page: Page, text: string | RegExp) {
  return page.locator("[data-sonner-toast]").filter({ hasText: text }).last();
}

/** "Friday 2 October, 15:50" from the preview "Friday 2 October, 15:50 · in 2 days". */
async function previewedTime(page: Page, dialogName: string, relative: RegExp): Promise<string> {
  const status = page.getByRole("dialog", { name: dialogName }).getByRole("status");
  await expect(status).toContainText(relative);
  const text = (await status.textContent()) ?? "";
  return text.split(" · ")[0]!.trim();
}

test('b then "in 2d" shows the time before it is set, names it in the toast, and u brings the row back', async ({
  page,
}) => {
  const row = (await laneRows(page, "owed"))[0]!;
  await openApp(page, "/desk?lane=owed");
  await expect(mailRows(page).first()).toBeVisible();
  await expectCursorOn(page, domId(row));

  await page.keyboard.press("b");
  const dialog = page.getByRole("dialog", { name: "Reply later" });
  await expect(dialog).toBeVisible();
  // The field has focus: typing goes straight in.
  await page.keyboard.type("in 2d");
  const shown = await previewedTime(page, "Reply later", /· in 2 days$/);

  const request = page.waitForRequest((r) => r.url().includes("/api/v1/mail/desk/later"));
  const response = page.waitForResponse("**/api/v1/mail/desk/later");
  await page.keyboard.press("Enter");
  const sent = (await request).postDataJSON() as { thread_ids: string[]; until: string };
  // The previewed instant, never the words.
  expect(sent.thread_ids).toEqual([row.thread_id]);
  expect(Date.parse(sent.until) - Date.now()).toBeGreaterThan(47 * 3_600_000);
  const body = (await (await response).json()) as {
    items: { kind: string }[];
    mutation_id?: string;
  };
  expect(body.items[0]?.kind).toBe("reply_later");
  expect(body.mutation_id).toBeTruthy();

  await expect(dialog).toHaveCount(0);
  await expect(lastToast(page, "Reply later: back")).toContainText(shown);
  await expect(rowById(page, domId(row))).toHaveCount(0);
  expect((await laneRows(page, "owed")).map((r) => r.thread_id)).not.toContain(row.thread_id);

  const undone = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await page.keyboard.press("u");
  expect((await undone).ok()).toBe(true);
  await expect(rowById(page, domId(row))).toBeVisible();
});

test('b on Waiting on then "in 3d" brings it back only if nobody replies, and the toast Undo restores it', async ({
  page,
}) => {
  const row = (await laneRows(page, "waiting"))[0]!;
  await openApp(page, "/desk?lane=waiting");
  await expect(mailRows(page).first()).toBeVisible();
  await expectCursorOn(page, domId(row));

  await page.keyboard.press("b");
  const name = "Bring back if nobody replies";
  const dialog = page.getByRole("dialog", { name });
  await expect(dialog).toBeVisible();
  await page.keyboard.type("in 3d");
  const shown = await previewedTime(page, name, /· in 3 days$/);

  const response = page.waitForResponse("**/api/v1/mail/desk/later");
  await page.keyboard.press("Enter");
  const body = (await (await response).json()) as { items: { kind: string }[] };
  expect(body.items[0]?.kind).toBe("waiting");
  const toast = lastToast(page, "if nobody replies");
  await expect(toast).toContainText(`Back ${shown} if nobody replies`);
  await expect(rowById(page, domId(row))).toHaveCount(0);

  const undone = page.waitForResponse("**/api/v1/mail/mutations/undo");
  await toast.getByRole("button", { name: "Undo" }).click();
  expect((await undone).ok()).toBe(true);
  await expect(rowById(page, domId(row))).toBeVisible();
});

test("a set time brings the conversation back on its own, after a daemon restart", async ({
  page,
}) => {
  const state = readE2EState();
  const row = (await laneRows(page, "owed"))[1]!;
  // A few seconds out, so the return happens while the test watches.
  // Whole seconds: times are stored to the second, as every picked time is.
  const until = new Date(Math.ceil((Date.now() + 8_000) / 1_000) * 1_000).toISOString();
  const set = await page.request.post(`${state.bridgeUrl}/api/v1/mail/desk/later`, {
    headers: auth(),
    data: { thread_ids: [row.thread_id], until },
  });
  expect(set.ok()).toBe(true);
  expect((await laneRows(page, "owed")).map((r) => r.thread_id)).not.toContain(row.thread_id);

  // The time and the return live in the store, not the process.
  await restartDaemon();

  await expect
    .poll(
      async () => {
        const back = await laneRows(page, "owed").catch(() => []);
        return back.find((r) => r.thread_id === row.thread_id)?.reason;
      },
      { timeout: 30_000, intervals: [1_000] },
    )
    .toBe("back from reply later");
  expect(Date.now()).toBeGreaterThanOrEqual(Date.parse(until));

  await openApp(page, "/desk?lane=owed");
  await expect(rowById(page, domId(row))).toContainText("back from reply later");
  const queue = await page.request.get(`${state.bridgeUrl}/api/v1/mail/reply-later`, {
    headers: auth(),
  });
  const { messages } = (await queue.json()) as { messages: { id: string }[] };
  expect(messages.map((message) => message.id)).toContain(row.message_id);

  // Leave the queue as the other specs expect it.
  await page.request.post(
    `${state.bridgeUrl}/api/v1/mail/reply-later/${encodeURIComponent(row.message_id)}`,
    { headers: auth(), data: { flag: false } },
  );
});
