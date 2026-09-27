import { expect, test, type Page, type Route } from "@playwright/test";

import { openList, pressSequence } from "./helpers/mail";
import { openApp, readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

// The desk refetches after sends; a stubbed refetch still in flight when a
// test ends must not fail it.
test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

const auth = () => ({ Authorization: `Bearer ${readE2EState().token}` });

interface OwedRow {
  thread_id: string;
  subject: string;
}

interface DeskAnswer {
  owed: { rows: OwedRow[]; total: number };
}

/**
 * Serve the desk with its You owe lane cut to `rows` (and `total`), every
 * other lane as the daemon has it. Focus mode's queue is that lane.
 */
async function deskOwes(page: Page, rows: () => OwedRow[], total?: () => number) {
  await page.route("**/api/v1/mail/desk?**", async (route: Route) => {
    const upstream = await route.fetch();
    const json = (await upstream.json()) as DeskAnswer;
    const ids = new Set(rows().map((row) => row.thread_id));
    const kept = json.owed.rows.filter((row) => ids.has(row.thread_id));
    await route.fulfill({
      response: upstream,
      json: {
        ...json,
        owed: {
          rows: kept,
          total: total?.() ?? kept.length,
        },
      },
    });
  });
}

/**
 * Each test works on the desk's first three owed conversations (real
 * rows, the lane cut short) and an empty reply-later queue.
 */
async function queueOfThree(page: Page): Promise<OwedRow[]> {
  const response = await page.request.get(
    `${readE2EState().bridgeUrl}/api/v1/mail/desk?lane_limit=5000`,
    { headers: auth() },
  );
  const { owed } = (await response.json()) as DeskAnswer;
  const three = owed.rows.slice(0, 3);
  expect(three).toHaveLength(3);
  await deskOwes(page, () => three);
  await page.route("**/api/v1/mail/reply-later", (route) =>
    route.fulfill({ json: { kind: "ReplyQueue", messages: [] } }),
  );
  return three;
}

function reply(page: Page) {
  return page.locator("[data-compose-surface]");
}

function heading(page: Page) {
  return page.getByRole("region", { name: "Conversation" }).getByRole("heading", { level: 1 });
}

/**
 * CodeMirror reads Mod from navigator.platform, which on a macOS host stays
 * Mac even under the Desktop Chrome device; the app's own keys follow the
 * user agent (see modKey). Keys pressed inside the editor use CodeMirror's.
 */
async function editorModKey(page: Page): Promise<"Meta" | "Control"> {
  return (await page.evaluate(() => /mac/i.test(navigator.platform))) ? "Meta" : "Control";
}

/** The reply body is CodeMirror in vim mode: insert, type, back to normal. */
async function typeReply(page: Page, text: string) {
  await reply(page).locator(".cm-content").click();
  await page.keyboard.press("i");
  await page.keyboard.type(text);
  await page.keyboard.press("Escape");
}

async function wakeSnoozed(page: Page) {
  const state = readE2EState();
  const response = await page.request.get(`${state.bridgeUrl}/api/v1/mail/snoozed`, {
    headers: auth(),
  });
  const { snoozed } = (await response.json()) as { snoozed: { message_id: string }[] };
  for (const entry of snoozed) {
    await page.request.post(
      `${state.bridgeUrl}/api/v1/mail/snoozed/${encodeURIComponent(entry.message_id)}/wake`,
      { headers: auth() },
    );
  }
}

test("g F works through the queue: send and next, skip, snooze, then a calm finish", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const three = await queueOfThree(page);
  await openList(page, "/m/inbox");
  await pressSequence(page, "g", "F");
  await expect(page).toHaveURL(/\/focus\?from=%2Fm%2Finbox$/);

  const progress = page.getByTestId("focus-progress");
  await expect(progress).toContainText("1 of 3");
  await expect(heading(page)).toHaveText(three[0]!.subject);
  // The conversation's context sits beside the reply, which opens itself.
  await expect(page.getByRole("region", { name: "Your reply" }).locator(reply(page))).toBeVisible();

  // Send and next, from the keyboard.
  await typeReply(page, "Thanks, that works for me.");
  const sent = page.waitForResponse("**/api/v1/mail/compose/session/send", { timeout: 30_000 });
  await page.keyboard.press(`${await editorModKey(page)}+Enter`);
  await expect(page.getByTestId("send-countdown")).toHaveText(/^Sending in \d+s$/);
  await expect(page.getByTestId("send-countdown-bar")).toBeVisible();
  await expect(progress).toContainText("2 of 3");
  await expect(heading(page)).toHaveText(three[1]!.subject);
  await expect(page.getByTestId("focus-progress-bar")).toHaveAttribute("style", /scaleX\(0\.333/);
  expect((await sent).ok()).toBe(true);

  // Skip keeps it in the queue, at the end: Tab out of the reply, then s.
  await reply(page).locator(".cm-content").click();
  await page.keyboard.press("Tab");
  await page.keyboard.press("s");
  await expect(heading(page)).toHaveText(three[2]!.subject);
  await expect(progress).toContainText("2 of 3");

  // Snooze with a typed time.
  await reply(page).locator(".cm-content").click();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Z");
  const snooze = page.getByRole("dialog", { name: "Snooze until…" });
  await snooze.getByLabel("Or type a time").fill("tomorrow 9am");
  await expect(snooze.getByRole("status")).toContainText("09:00");
  await snooze.getByLabel("Or type a time").press("Enter");
  await expect(snooze).toHaveCount(0);
  await expect(page.getByText(/^Snoozed .*until .+, 09:00$/)).toBeVisible();

  // The skipped one comes back; reply to it and the queue is done.
  await expect(heading(page)).toHaveText(three[1]!.subject);
  await expect(progress).toContainText("3 of 3");
  await typeReply(page, "Sorted, thank you.");
  await page.keyboard.press(`${await editorModKey(page)}+Enter`);
  const finish = page.getByTestId("focus-finish");
  await expect(finish).toBeVisible();
  await expect(finish).toContainText("That's everyone.");
  await expect(finish).toContainText("3 conversations handled");

  // Esc goes back where focus mode was opened from.
  await page.locator('[aria-label="Focus and reply"]').focus();
  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await wakeSnoozed(page);
});

test("undo inside the countdown restores the reply and puts the conversation back", async ({
  page,
}) => {
  test.setTimeout(60_000);
  const three = await queueOfThree(page);
  let sends = 0;
  page.on("request", (request) => {
    if (request.url().includes("/api/v1/mail/compose/session/send")) sends += 1;
  });
  await openApp(page, "/focus");
  await expect(heading(page)).toHaveText(three[0]!.subject);
  await typeReply(page, "Undo me please");
  await page.keyboard.press(`${await editorModKey(page)}+Enter`);
  await expect(page.getByTestId("focus-progress")).toContainText("2 of 3");

  await page.getByRole("button", { name: "Undo" }).click();
  await expect(page.getByText("Send cancelled")).toBeVisible();
  await expect(page.getByTestId("focus-progress")).toContainText("1 of 3");
  await expect(heading(page)).toHaveText(three[0]!.subject);
  await expect(reply(page).locator(".cm-content")).toContainText("Undo me please");

  // w: send with a reminder if nobody replies, with the time in words.
  await reply(page).locator(".cm-content").click();
  await page.keyboard.press("Tab");
  await page.keyboard.press("w");
  const remind = page.getByRole("dialog", { name: "Send and remind me" });
  await expect(remind).toBeVisible();
  await remind.getByLabel("Or type a time").fill("in 3 days");
  await expect(remind.getByRole("status")).toContainText(/\d{2}:\d{2}/);
  await page.keyboard.press("Escape");
  await expect(remind).toHaveCount(0);
  // Past the window: nothing went out.
  await page.waitForTimeout(11_000);
  expect(sends).toBe(0);
});

test("fast back and forth never loses the latest reply text", async ({ page }) => {
  test.setTimeout(60_000);
  const three = await queueOfThree(page);
  // Slow saves, so leaving A and coming back overtakes its save.
  await page.route("**/api/v1/mail/compose/session/update", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1500));
    await route.continue();
  });
  await openApp(page, "/focus");
  await expect(heading(page)).toHaveText(three[0]!.subject);
  await typeReply(page, "Latest words for A");
  // Straight out and through the queue back to A, without waiting for saves.
  for (const next of [three[1]!, three[2]!, three[0]!]) {
    await reply(page).locator(".cm-content").click();
    await page.keyboard.press("Tab");
    await page.keyboard.press("s");
    await expect(heading(page)).toHaveText(next.subject);
    await expect(reply(page).locator(".cm-content")).toBeVisible();
  }
  await expect(reply(page).locator(".cm-content")).toContainText("Latest words for A");
});

test("skipping the last one sets it aside, and you can come back to it", async ({ page }) => {
  const three = await queueOfThree(page);
  await page.unroute("**/api/v1/mail/desk?**");
  await deskOwes(page, () => [three[0]!]);
  await openApp(page, "/focus");
  await expect(page.getByTestId("focus-progress")).toContainText("1 of 1");
  await reply(page).locator(".cm-content").click();
  await page.keyboard.press("Tab");
  await page.keyboard.press("s");
  const finish = page.getByTestId("focus-finish");
  await expect(finish).toContainText("1 conversation skipped.");
  await expect(finish).not.toContainText("That's everyone");
  await finish.getByRole("button", { name: "Come back to it" }).click();
  await expect(heading(page)).toHaveText(three[0]!.subject);
});

test("a failed load is an error to retry, never a finish", async ({ page }) => {
  let fail = true;
  await page.route("**/api/v1/mail/desk?**", async (route) => {
    if (fail) return route.fulfill({ status: 500, json: { error: "daemon unavailable" } });
    const upstream = await route.fetch();
    const json = (await upstream.json()) as DeskAnswer;
    return route.fulfill({ response: upstream, json: { ...json, owed: { rows: [], total: 0 } } });
  });
  await page.route("**/api/v1/mail/reply-later", (route) =>
    route.fulfill({ json: { kind: "ReplyQueue", messages: [] } }),
  );
  await openApp(page, "/focus");
  await expect(page.getByTestId("focus-error")).toBeVisible({ timeout: 20_000 });
  await expect(page.getByText(/Nobody is waiting|That's everyone/)).toHaveCount(0);
  fail = false;
  await page.getByTestId("focus-error").getByRole("button", { name: "Try again" }).click();
  await expect(page.getByTestId("focus-finish")).toContainText("Nobody is waiting");
});

test("a cut-short lane says how many more are waiting, never that's everyone", async ({ page }) => {
  const three = await queueOfThree(page);
  await page.unroute("**/api/v1/mail/desk?**");
  await deskOwes(
    page,
    () => [three[0]!],
    () => 4,
  );
  await openApp(page, "/focus");
  await expect(page.getByTestId("focus-progress")).toContainText("1 of 1");
  await reply(page).locator(".cm-content").click();
  await page.keyboard.press("Tab");
  await page.keyboard.press("s");
  const finish = page.getByTestId("focus-finish");
  await expect(finish).toContainText("That's this batch.");
  await expect(finish).toContainText("3 more conversations waiting on a reply.");
  await expect(finish).not.toContainText("That's everyone");
  await expect(finish.getByRole("button", { name: "Continue with 3 more" })).toBeVisible();
});

test("a dated promise in a reply is offered with its time and kept as a reminder", async ({
  page,
}) => {
  test.setTimeout(90_000);
  const three = await queueOfThree(page);
  const state = readE2EState();
  const zone = await page.evaluate(() => Intl.DateTimeFormat().resolvedOptions().timeZone);
  // The due words resolve through the real parser; only the model is stubbed.
  const resolved = (await (
    await page.request.get(
      `${state.bridgeUrl}/api/v1/mail/time/resolve?input=Friday&time_zone=${encodeURIComponent(zone)}`,
      { headers: auth() },
    )
  ).json()) as {
    resolution: { at: string; choices: { date_label: string; time_label: string }[] };
  };
  const due = resolved.resolution;
  const choice = due.choices[0]!;
  await page.route("**/api/v1/mail/compose/session/promises", async (route) => {
    const body = route.request().postDataJSON() as { time_zone?: string };
    expect(body.time_zone).toBe(zone);
    await new Promise((resolve) => setTimeout(resolve, 600));
    await route.fulfill({
      json: {
        kind: "Promises",
        detection: {
          status: "ready",
          promises: [{ what: "send the deck", due_phrase: "by Friday", due }],
          provenance: { model: "stub-7b", locality: "local", sources: ["your_message"] },
        },
      },
    });
  });

  await openApp(page, "/focus");
  await expect(heading(page)).toHaveText(three[0]!.subject);
  await typeReply(page, "Happy to help. I'll send the deck by Friday.");
  const sent = page.waitForResponse("**/api/v1/mail/compose/session/send", { timeout: 30_000 });
  await page.keyboard.press(`${await editorModKey(page)}+Enter`);

  // Offered while the send is still in its undo window.
  const offer = page.getByTestId("promise-offer");
  await expect(offer).toContainText("You promised: send the deck (by Friday)");
  await expect(page.getByTestId("promise-offer-time")).toHaveText(
    `Remind me ${choice.date_label}, ${choice.time_label}?`,
  );
  await expect(page.getByTestId("send-countdown")).toHaveText(/^Sending in \d+s$/);
  await offer.getByRole("button", { name: "Remind me" }).click();
  await expect(offer).toContainText("kept once it sends");

  const sentJson = (await (await sent).json()) as { message_id: string };
  await expect(
    page.getByText(`Reminder set for ${choice.date_label}, ${choice.time_label}`),
  ).toBeVisible();

  // The daemon holds it as an open promise, due at exactly that instant.
  const accounts = (await (
    await page.request.get(`${state.bridgeUrl}/api/v1/platform/accounts`, { headers: auth() })
  ).json()) as { accounts: { account_id: string }[] };
  const list = (await (
    await page.request.get(
      `${state.bridgeUrl}/api/v1/mail/commitments?account_id=${accounts.accounts[0]!.account_id}&status=open`,
      { headers: auth() },
    )
  ).json()) as {
    commitments: { id: string; what: string; by_when?: string; evidence_msg_id: string }[];
  };
  const kept = list.commitments.find((row) => row.evidence_msg_id === sentJson.message_id);
  expect(kept).toMatchObject({ what: "send the deck", direction: "yours" });
  expect(Date.parse(kept!.by_when!)).toBe(Date.parse(due.at));
  await page.request.post(
    `${state.bridgeUrl}/api/v1/mail/commitments/${encodeURIComponent(kept!.id)}/resolve`,
    { headers: auth() },
  );
});

test("focus mode reads well on a phone", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const three = await queueOfThree(page);
  await openApp(page, "/focus");
  await expect(heading(page)).toHaveText(three[0]!.subject);
  await expect(reply(page)).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
  // Stacked: the conversation ends before the reply starts.
  // Measured once layout settles: the thread and the reply load separately.
  await expect
    .poll(async () => {
      const conversation = await page.getByRole("region", { name: "Conversation" }).boundingBox();
      const replyBox = await page.getByRole("region", { name: "Your reply" }).boundingBox();
      return conversation && replyBox
        ? conversation.y + conversation.height <= replyBox.y + 1
        : false;
    })
    .toBe(true);
});

test("the desk's You owe lane opens focus mode on those replies", async ({ page }) => {
  const three = await queueOfThree(page);
  await openApp(page, "/desk");
  const action = page.getByTestId("group-action-owed");
  await expect(action).toContainText(/Reply to (all \d+|them) in focus mode/);
  await action.click();
  await expect(page).toHaveURL(/\/focus\?lane=owed&from=%2Fdesk$/);
  await expect(heading(page)).toHaveText(three[0]!.subject);

  // Esc goes back to the desk, and g F from there opens the same lane.
  await page.locator('[aria-label="Focus and reply"]').focus();
  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(/\/desk$/);
  await pressSequence(page, "g", "F");
  await expect(page).toHaveURL(/\/focus\?lane=owed&from=%2Fdesk$/);
});
