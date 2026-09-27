import { expect, test, type Page } from "@playwright/test";

import { cursorRowId, mailList, openList, pressSequence, reader, rowById } from "./helpers/mail";
import { openApp, readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test('"fri 3" offers 15:00 and 03:00, and the snooze stores the time the preview showed', async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  await cursorToSingleMessageRow(page);
  await page.keyboard.press("Enter");
  await expect(reader(page)).toBeVisible();

  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
  const field = dialog.getByLabel("Or type a time");
  await field.fill("fri 3");

  const chips = dialog
    .getByRole("radiogroup", { name: "Which time did you mean?" })
    .getByRole("radio");
  await expect(chips).toHaveCount(2);
  await expect(chips.nth(0)).toContainText("15:00");
  await expect(chips.nth(1)).toContainText("03:00");
  await expect(chips.nth(0)).toHaveAttribute("aria-checked", "true");
  await expect(dialog.locator("mark[data-understood]")).toHaveText("fri 3");
  const status = dialog.getByRole("status");
  await expect(status).toContainText(/^Friday \d+ \w+, 15:00 · in \d+ days?$/);
  // The am/pm was assumed, so the time is muted.
  await expect(status.locator("[data-assumed]")).toHaveText("15:00");

  // Try the other reading, then come back to 15:00 and commit from the chips.
  await chips.nth(1).click();
  await expect(status).toContainText("03:00");
  await chips.nth(1).press("ArrowLeft");
  await expect(chips.nth(0)).toHaveAttribute("aria-checked", "true");

  const snoozeRequest = page.waitForRequest(
    (request) =>
      request.url().includes("/api/v1/mail/actions/snooze") && request.method() === "POST",
  );
  await chips.nth(0).press("Enter");
  const until = ((await snoozeRequest).postDataJSON() as { until: string }).until;
  await expect(dialog).toHaveCount(0);

  // What was stored is an instant, on a Friday at 15:00 local time.
  const local = await page.evaluate((iso) => {
    const date = new Date(iso);
    return { day: date.getDay(), hour: date.getHours(), minute: date.getMinutes() };
  }, until);
  expect(local).toEqual({ day: 5, hour: 15, minute: 0 });

  // The daemon reports the same wake time.
  const state = readE2EState();
  const response = await page.request.get(`${state.bridgeUrl}/api/v1/mail/snoozed`, {
    headers: { Authorization: `Bearer ${state.token}` },
  });
  const { snoozed } = (await response.json()) as { snoozed: { wake_at: string }[] };
  expect(snoozed.map((entry) => Date.parse(entry.wake_at))).toContain(Date.parse(until));

  // And the Snoozed view shows it.
  await pressSequence(page, "g", "n");
  await expect(page).toHaveURL(/\/snoozed$/);
  const shown = await page.evaluate(
    (iso) =>
      new Intl.DateTimeFormat(undefined, {
        weekday: "short",
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
      }).format(new Date(iso)),
    until,
  );
  await expect(mailList(page).getByText(shown, { exact: false }).first()).toBeVisible();
});

test("a phrase it can't read says which word, calmly", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
  await dialog.getByLabel("Or type a time").fill("frday 3");
  await expect(dialog.getByRole("status")).toHaveText(
    'Didn\'t catch "frday". Try "fri 3pm" or "in 2d".',
  );
  await expect(dialog.getByRole("button", { name: "Snooze", exact: true })).toBeDisabled();
});

test("reduced motion keeps the fade and drops the scale", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openList(page, "/m/inbox");
  expect(await page.evaluate(() => document.documentElement.dataset.motion)).toBe("reduced");

  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
  await expect(dialog).toBeVisible();
  const motion = await dialog.evaluate((node) => {
    const style = getComputedStyle(node);
    return {
      animation: style.animationName,
      scale: style.getPropertyValue("--tw-enter-scale").trim(),
    };
  });
  // The dialog still runs its enter fade, but no longer grows.
  expect(motion).toEqual({ animation: "enter", scale: "1" });
});

test("full motion scales dialogs in, and rows never animate the keyboard cursor", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await openList(page, "/m/inbox");
  expect(await page.evaluate(() => document.documentElement.dataset.motion)).toBe("full");

  const rowId = await cursorRowId(page);
  const rowTransition = await rowById(page, rowId).evaluate(
    (node) => getComputedStyle(node).transitionDuration,
  );
  expect(rowTransition).toBe("0s");

  await page.keyboard.press("Z");
  const dialog = page.getByRole("dialog", { name: "Snooze until…" });
  await expect(dialog).toBeVisible();
  const scale = await dialog.evaluate((node) =>
    getComputedStyle(node).getPropertyValue("--tw-enter-scale").trim(),
  );
  expect(scale).not.toBe("1");
  expect(scale).not.toBe("");
});

test("the Motion setting overrides the system preference", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await openApp(page, "/settings/appearance");
  await page.getByRole("combobox", { name: "Motion" }).click();
  await page.getByRole("option", { name: "Reduced" }).click();
  expect(await page.evaluate(() => document.documentElement.dataset.motion)).toBe("reduced");
});

async function cursorToSingleMessageRow(page: Page): Promise<string> {
  for (let step = 0; step < 15; step += 1) {
    const id = await cursorRowId(page);
    const name = (await rowById(page, id).getAttribute("aria-label")) ?? "";
    if (!/messages in conversation/.test(name)) return id;
    await page.keyboard.press("j");
  }
  throw new Error("no single-message conversation in the first rows");
}
