import { expect, test, type Page } from "@playwright/test";

import {
  expectCursorOn,
  mailRows,
  pressSequence,
  reader,
  renderedRowIds,
  rowById,
  threadMessages,
} from "./helpers/mail";
import { openApp, restartDaemon, stopDaemon } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

// Recovery waits on the daemon's own restart plus one probe (at most 8s
// apart, with jitter), so it gets a wider window than a normal assertion.
const RECOVERY_MS = 30_000;

function daemonBanner(page: Page) {
  return page.locator('[data-offline-banner="daemon"]');
}

function inlineComposer(page: Page) {
  return page.locator("#inline-composer-slot [data-compose-surface]");
}

/** A value on `window` survives only if the page was never reloaded. */
async function markPage(page: Page): Promise<void> {
  await page.evaluate(() => {
    (window as Window & { mxrNoReload?: boolean }).mxrNoReload = true;
  });
}

async function pageWasNotReloaded(page: Page): Promise<boolean> {
  return page.evaluate(() => (window as Window & { mxrNoReload?: boolean }).mxrNoReload === true);
}

test("with the daemon stopped, loaded mail stays readable and keys move; archive is refused; it all comes back without a reload", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const requests: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("/api/")) requests.push(request.url());
  });

  // Choose and open Messages' first conversation, then load the inbox.
  await openApp(page, "/messages");
  await expect(page).toHaveURL(/\/messages$/);
  await expect(page.getByTestId("messages-row").first()).toBeVisible();
  const openedPersonId = await page.getByTestId("messages-row").first().getAttribute("data-row-id");
  expect(openedPersonId).toBeTruthy();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("conversation-message").first()).toBeVisible();
  const openedPerson = await page.getByTestId("person-name").textContent();
  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  // Messages' rows can linger for a frame after the route changes.
  await expect(page.getByRole("heading", { level: 1, name: "Inbox", exact: true })).toBeVisible();
  await expect.poll(async () => (await renderedRowIds(page)).length).toBeGreaterThan(1);
  const [first, second] = await renderedRowIds(page);
  await markPage(page);

  await stopDaemon();
  let restarted = false;
  try {
    await expect(daemonBanner(page)).toBeVisible({ timeout: 10_000 });
    await expect(daemonBanner(page)).toContainText("You can keep reading what's loaded");

    // The inbox stays on screen and the cursor still moves.
    await expect(rowById(page, first!)).toBeVisible();
    await expectCursorOn(page, first!);
    await page.keyboard.press("j");
    await expectCursorOn(page, second!);
    await page.keyboard.press("k");
    await expectCursorOn(page, first!);

    // Archive is refused up front: the row stays, and the toast says why.
    await page.keyboard.press("e");
    await expect(page.getByText("Can't archive while mxr's daemon is stopped")).toBeVisible();
    await expect(page.getByText("Nothing changed. Try again once it's back.")).toBeVisible();
    await expect(rowById(page, first!)).toBeVisible();
    await expect(page.getByText(/^Archived/)).toHaveCount(0);

    // A g-jump back to Messages leaves the person closed. Opening that row
    // explicitly still shows the conversation from cache while offline.
    await pressSequence(page, "g", "m");
    await expect(page).toHaveURL(/\/messages/);
    await expect(page.getByTestId("messages-row").first()).toBeVisible();
    await expect(page.getByTestId("person-page")).toHaveCount(0);
    await expect(page.getByTestId("conversation-message")).toHaveCount(0);
    await page.locator(`[data-testid="messages-row"][data-row-id="${openedPersonId}"]`).click();
    await expect(page.getByTestId("person-name")).toHaveText(openedPerson!);
    await expect(page.getByTestId("conversation-message").first()).toBeVisible();
    await pressSequence(page, "g", "i");
    await expect(rowById(page, first!)).toBeVisible();

    // No retry storm: a few seconds down costs a handful of probes.
    const before = requests.length;
    await page.waitForTimeout(5_000);
    expect(requests.length - before).toBeLessThanOrEqual(4);

    await restartDaemon();
    restarted = true;
  } finally {
    if (!restarted) await restartDaemon();
  }

  await expect(daemonBanner(page)).toBeHidden({ timeout: RECOVERY_MS });
  await expect(page.getByRole("contentinfo").getByText(/^connected$/i)).toBeVisible({
    timeout: RECOVERY_MS,
  });
  expect(await pageWasNotReloaded(page)).toBe(true);

  // Actions work again: archive, then undo to leave the mailbox as it was.
  await expectCursorOn(page, first!);
  await page.keyboard.press("e");
  await expect(rowById(page, first!)).toHaveCount(0);
  await expect(page.getByText(/^Archived \d+ messages?$/)).toBeVisible({ timeout: 15_000 });
  await page.keyboard.press("u");
  await expect(page.getByText(/^Undone$/)).toBeVisible();
  await expect(rowById(page, first!)).toBeVisible();
});

test("a reply typed while the daemon is stopped keeps its text and saves once it is back", async ({
  page,
}) => {
  test.setTimeout(120_000);
  await openApp(page, "/m/inbox");
  await expect(mailRows(page).first()).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(threadMessages(page).first()).toBeVisible();
  await page.keyboard.press("r");
  const body = inlineComposer(page).locator(".cm-content");
  await expect(body).toBeVisible();
  await body.click();
  await page.keyboard.press("i");
  await page.keyboard.type("Written before the stop.");
  await expect(inlineComposer(page).getByText(/^Saved/)).toBeVisible({ timeout: 10_000 });
  await markPage(page);

  await stopDaemon();
  let restarted = false;
  try {
    await expect(daemonBanner(page)).toBeVisible({ timeout: 10_000 });
    await page.keyboard.type(" Written while it was stopped.");
    // Past the autosave debounce: nothing could save, the text is all there.
    await page.waitForTimeout(4_000);
    await expect(
      inlineComposer(page).getByText(/Not saved: mxr's daemon is stopped/),
    ).toBeVisible();
    await expect(body).toContainText("Written before the stop. Written while it was stopped.");

    await restartDaemon();
    restarted = true;
  } finally {
    if (!restarted) await restartDaemon();
  }

  await expect(daemonBanner(page)).toBeHidden({ timeout: RECOVERY_MS });
  await expect(inlineComposer(page).getByText(/^Saved/)).toBeVisible({ timeout: RECOVERY_MS });
  await expect(body).toContainText("Written before the stop. Written while it was stopped.");
  expect(await pageWasNotReloaded(page)).toBe(true);
});

test("an event stream that keeps failing backs off and pauses nothing that doesn't need it", async ({
  page,
}) => {
  // What a bridge that can't reach the daemon's events does: accept, say
  // why, close. Every attempt is counted.
  let attempts = 0;
  await page.routeWebSocket(/\/api\/v1\/events/, (ws) => {
    attempts += 1;
    ws.send(JSON.stringify({ error: "failed to connect to mxr daemon" }));
    ws.close();
  });
  await openApp(page, "/m/inbox");
  await expect(mailRows(page).first()).toBeVisible();
  await page.waitForTimeout(6_000);
  // 250ms doubling with jitter: about five tries in six seconds, not 24.
  expect(attempts).toBeLessThanOrEqual(7);
  await expect(page.getByRole("contentinfo").getByText(/^reconnecting$/i)).toBeVisible();

  // Requests still reach the daemon, so nothing is refused.
  await expect(daemonBanner(page)).toHaveCount(0);
  const [first] = await renderedRowIds(page);
  await expectCursorOn(page, first!);
  await page.keyboard.press("e");
  await expect(rowById(page, first!)).toHaveCount(0);
  await expect(page.getByText(/^Archived \d+ messages?$/)).toBeVisible({ timeout: 15_000 });
  await page.keyboard.press("u");
  await expect(page.getByText(/^Undone$/)).toBeVisible();
});

async function startNewMessage(page: Page, subject: string): Promise<void> {
  await openApp(page, "/m/inbox");
  await expect(mailRows(page).first()).toBeVisible();
  await page.keyboard.press("c");
  const composer = page.getByRole("dialog", { name: "New message" });
  await composer.getByRole("combobox", { name: "To" }).fill("alice@example.com");
  await composer.getByRole("textbox", { name: "Subject" }).fill(subject);
  await composer.locator(".cm-content").click();
  await page.keyboard.press("i");
  await page.keyboard.type("Only if I press Send again.");
  await page.keyboard.press("Escape");
  await expect(composer.getByText(/^Saved/)).toBeVisible({ timeout: 10_000 });
}

function countSends(page: Page): () => number {
  let sends = 0;
  page.on("request", (request) => {
    if (request.url().includes("/api/v1/mail/compose/session/send")) sends += 1;
  });
  return () => sends;
}

test("confirming the send dialog while the daemon is stopped sends nothing, then or after it returns", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const sends = countSends(page);
  await startNewMessage(page, `e2e-down-confirm-${Date.now().toString(36)}`);
  // A failed safety check opens the confirm dialog instead of sending.
  await page.route("**/api/v1/mail/compose/session/safety-check", (route) =>
    route.fulfill({ status: 500, json: { error: "checker unavailable", code: "internal" } }),
  );
  const composer = page.getByRole("dialog", { name: "New message" });
  await composer.getByRole("button", { name: /^Send (⌘|Ctrl)/ }).click();
  const confirm = page.getByRole("alertdialog");
  await expect(confirm.getByText(/Safety check unavailable/)).toBeVisible();

  await stopDaemon();
  let restarted = false;
  try {
    await expect(daemonBanner(page)).toBeVisible({ timeout: 10_000 });
    await confirm.getByRole("button", { name: /^Send$/ }).click();
    await expect(page.getByText("Can't send while mxr's daemon is stopped")).toBeVisible();
    await expect(page.getByText(/^Sending in \d+s/)).toHaveCount(0);
    await restartDaemon();
    restarted = true;
  } finally {
    if (!restarted) await restartDaemon();
  }
  await expect(daemonBanner(page)).toBeHidden({ timeout: RECOVERY_MS });
  // Past where a 10 s undo window would have fired.
  await page.waitForTimeout(12_000);
  expect(sends()).toBe(0);
});

test("a send whose undo window closes while the daemon is stopped is cancelled, not replayed", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const sends = countSends(page);
  await startNewMessage(page, `e2e-down-window-${Date.now().toString(36)}`);
  const composer = page.getByRole("dialog", { name: "New message" });
  await composer.getByRole("button", { name: /^Send (⌘|Ctrl)/ }).click();
  await expect(page.getByText(/^Sending in \d+s/)).toBeVisible();

  await stopDaemon();
  let restarted = false;
  try {
    await expect(daemonBanner(page)).toBeVisible({ timeout: 10_000 });
    // The 10 s window closes while the daemon is still down.
    await expect(page.getByText("Can't send while mxr's daemon is stopped")).toBeVisible({
      timeout: 15_000,
    });
    await restartDaemon();
    restarted = true;
  } finally {
    if (!restarted) await restartDaemon();
  }
  await expect(daemonBanner(page)).toBeHidden({ timeout: RECOVERY_MS });
  await page.waitForTimeout(3_000);
  expect(sends()).toBe(0);
});
