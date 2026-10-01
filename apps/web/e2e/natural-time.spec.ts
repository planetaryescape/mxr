import { expect, test, type Page } from "@playwright/test";

import { cursorRowId, mailList, openList, pressSequence, reader, rowById } from "./helpers/mail";
import { openApp, readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

// Snoozes made here must not leak into other specs that share the daemon.
test.afterEach(async ({ request }) => {
  const state = readE2EState();
  const headers = { Authorization: `Bearer ${state.token}` };
  const response = await request.get(`${state.bridgeUrl}/api/v1/mail/snoozed`, { headers });
  const { snoozed } = (await response.json()) as { snoozed: { message_id: string }[] };
  for (const entry of snoozed) {
    await request.post(
      `${state.bridgeUrl}/api/v1/mail/snoozed/${encodeURIComponent(entry.message_id)}/wake`,
      { headers },
    );
  }
});

/**
 * Resolve every phrase as if it were Monday 10:00 local time next week, so
 * "fri 3" is always four days out whatever day the suite runs (on a
 * Thursday it would read "tomorrow"). Next week keeps the snooze in the
 * future for the daemon. Only `now` is added; the real parser answers.
 */
async function pinResolveClockToNextMonday(page: Page): Promise<void> {
  const monday = new Date();
  monday.setDate(monday.getDate() + ((8 - monday.getDay()) % 7 || 7));
  monday.setHours(10, 0, 0, 0);
  await page.route("**/api/v1/mail/time/resolve?**", async (route) => {
    const url = new URL(route.request().url());
    url.searchParams.set("now", monday.toISOString());
    await route.continue({ url: url.toString() });
  });
}

test('"fri 3" offers 15:00 and 03:00, and the snooze stores the time the preview showed', async ({
  page,
}) => {
  await pinResolveClockToNextMonday(page);
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
  await expect(status).toContainText(/^Friday \d+ \w+, 15:00 · in 4 days$/);
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

test.describe("a browser in another zone than the daemon", () => {
  test.use({ timezoneId: "Pacific/Kiritimati" });

  test('"fri 3pm" means 15:00 where the browser is', async ({ page }) => {
    await openList(page, "/m/inbox");
    await page.keyboard.press("Z");
    const dialog = page.getByRole("dialog", { name: "Snooze until…" });
    await dialog.getByLabel("Or type a time").fill("fri 3pm");
    await expect(dialog.getByRole("status")).toContainText("15:00");

    const snoozeRequest = page.waitForRequest(
      (request) =>
        request.url().includes("/api/v1/mail/actions/snooze") && request.method() === "POST",
    );
    await dialog.getByLabel("Or type a time").press("Enter");
    const until = ((await snoozeRequest).postDataJSON() as { until: string }).until;
    // Kiritimati is UTC+14, so 15:00 there is 01:00 UTC the same day.
    expect(new Date(until).getUTCHours()).toBe(1);
    await expect(page.getByText(/^Snoozed until Friday \d+ \w+, 15:00$/)).toBeVisible();
  });
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

  // Spinners pulse instead of turning, and nothing running moves.
  expect(await mountSpinner(page)).toBe("pending-pulse");
  const moving = await runningTransformAnimations(page);
  expect(moving).toEqual([]);
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

  // The detector used in the reduced-motion test does see a turning spinner.
  expect(await mountSpinner(page)).toBe("spin");
  expect(await runningTransformAnimations(page)).toContain("spin");
});

test("the Motion setting overrides the system preference", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await openApp(page, "/settings/appearance");
  await page.getByRole("combobox", { name: "Motion" }).click();
  await page.getByRole("option", { name: "Reduced" }).click();
  expect(await page.evaluate(() => document.documentElement.dataset.motion)).toBe("reduced");
});

/** Add a Tailwind spinner to the page; returns the animation it runs. */
async function mountSpinner(page: Page): Promise<string> {
  return page.evaluate(() => {
    const node = document.createElement("span");
    node.className = "animate-spin";
    document.body.append(node);
    return getComputedStyle(node).animationName;
  });
}

/**
 * Names of running animations that move their element: each is paused and
 * sampled part-way through, and a non-identity transform means movement.
 */
async function runningTransformAnimations(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const identity = new Set(["none", "matrix(1, 0, 0, 1, 0, 0)"]);
    return document
      .getAnimations()
      .filter((animation) => animation.playState === "running")
      .flatMap((animation) => {
        const effect = animation.effect;
        const target = effect instanceof KeyframeEffect ? effect.target : null;
        const duration = Number(effect?.getComputedTiming().duration ?? 0);
        if (!(target instanceof Element) || !duration) return [];
        const resume = animation.currentTime;
        animation.pause();
        const moves = [0.25, 0.5, 0.75].some((fraction) => {
          animation.currentTime = duration * fraction;
          return !identity.has(getComputedStyle(target).transform);
        });
        animation.currentTime = resume;
        animation.play();
        return moves
          ? [animation instanceof CSSAnimation ? animation.animationName : "unnamed"]
          : [];
      });
  });
}

async function cursorToSingleMessageRow(page: Page): Promise<string> {
  for (let step = 0; step < 15; step += 1) {
    const id = await cursorRowId(page);
    const name = (await rowById(page, id).getAttribute("aria-label")) ?? "";
    if (!/messages in conversation/.test(name)) return id;
    await page.keyboard.press("j");
  }
  throw new Error("no single-message conversation in the first rows");
}
