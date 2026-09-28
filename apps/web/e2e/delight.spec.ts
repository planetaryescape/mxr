import { expect, test, type Page, type Route } from "@playwright/test";

import { mailRows, openList, pressSequence } from "./helpers/mail";
import { openApp, readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

interface DeskRowJson {
  id: string;
  thread_id: string;
}

type Lane = "owed" | "due" | "waiting" | "people_new";
const LANES: Lane[] = ["owed", "due", "waiting", "people_new"];

/**
 * Cut the desk down to one owed conversation, as the daemon has it: once
 * that conversation is archived the daemon drops it and the desk is clear.
 */
async function deskOfOne(page: Page): Promise<DeskRowJson> {
  const response = await page.request.get(`${readE2EState().bridgeUrl}/api/v1/mail/desk`, {
    headers: { Authorization: `Bearer ${readE2EState().token}` },
  });
  const desk = (await response.json()) as Record<Lane, { rows: DeskRowJson[] }>;
  const keep = desk.owed.rows[0];
  expect(keep).toBeTruthy();
  await page.route("**/api/v1/mail/desk?**", async (route: Route) => {
    const upstream = await route.fetch();
    const json = (await upstream.json()) as Record<string, unknown>;
    for (const lane of LANES) {
      const rows = ((json[lane] as { rows: DeskRowJson[] }).rows ?? []).filter(
        (row) => row.thread_id === keep!.thread_id,
      );
      json[lane] = { ...(json[lane] as object), rows, total: rows.length };
    }
    await route.fulfill({ response: upstream, json });
  });
  return keep!;
}

/** Animations running inside the low tide scene. */
function sceneAnimations(page: Page): Promise<number> {
  return page.evaluate(
    () =>
      document
        .getAnimations()
        .filter(
          (animation) =>
            animation.playState === "running" &&
            animation.effect instanceof KeyframeEffect &&
            animation.effect.target instanceof Element &&
            animation.effect.target.closest(".low-tide") !== null,
        ).length,
  );
}

test("clearing the desk by keyboard earns low tide once, not on a revisit", async ({ page }) => {
  await deskOfOne(page);
  await openList(page, "/desk");
  await expect(mailRows(page)).toHaveCount(1);
  await expect(page.getByTestId("low-tide")).toHaveCount(0);
  await page.keyboard.press("e");
  const moment = page.getByTestId("low-tide");
  await expect(moment).toBeVisible();
  await expect(moment).toContainText("Low tide. Nobody's waiting on you.");
  // The scene moves, then is still: under two seconds of motion.
  await expect.poll(() => sceneAnimations(page), { timeout: 3000 }).toBe(0);

  await pressSequence(page, "g", "i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await expect(mailRows(page).first()).toBeVisible();
  await pressSequence(page, "g", "d");
  await expect(page).toHaveURL(/\/desk$/);
  await expect(page.getByText("The desk is clear.")).toBeVisible();
  await expect(page.getByTestId("low-tide")).toHaveCount(0);

  // Put the conversation back for the rest of the suite.
  await page.keyboard.press("u");
  await expect(page.getByText(/^Undone$/)).toBeVisible();
});

test("with reduced motion the low tide is a still frame", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await deskOfOne(page);
  await openList(page, "/desk");
  await expect(mailRows(page)).toHaveCount(1);
  await page.keyboard.press("e");
  await expect(page.getByTestId("low-tide-scene")).toBeVisible();
  expect(await sceneAnimations(page)).toBe(0);
  const transform = await page
    .locator(".low-tide .tide-water")
    .evaluate((node) => getComputedStyle(node).transform);
  expect(transform === "none" || transform === "matrix(1, 0, 0, 1, 0, 0)").toBe(true);
  await page.keyboard.press("u");
  await expect(page.getByText(/^Undone$/)).toBeVisible();
});

/** Record Web Audio use instead of making sound. */
async function stubAudio(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const log: string[] = [];
    (window as unknown as { __audio: string[] }).__audio = log;
    const param = { setValueAtTime() {}, exponentialRampToValueAtTime() {} };
    class FakeAudioContext {
      state = "running";
      currentTime = 0;
      destination = {};
      resume() {
        return Promise.resolve();
      }
      createOscillator() {
        return {
          type: "sine",
          frequency: param,
          connect() {},
          start() {
            log.push("start");
          },
          stop() {},
        };
      }
      createGain() {
        return { gain: param, connect() {} };
      }
    }
    (window as unknown as { AudioContext: unknown }).AudioContext = FakeAudioContext;
  });
}

const audioStarts = (page: Page) =>
  page.evaluate(() => (window as unknown as { __audio: string[] }).__audio.length);

test("sound: off by default, previews in the browser, plays on archive but never on j", async ({
  page,
}) => {
  // The real route reads the shared daemon setting, off by default.
  const current = await page.request.get(
    `${readE2EState().bridgeUrl}/api/v1/platform/notifications/chimes`,
    { headers: { Authorization: `Bearer ${readE2EState().token}` } },
  );
  expect(current.ok()).toBe(true);
  const setting = (await current.json()) as { kind: string; config: { enabled: boolean } };
  expect(setting.kind).toBe("NotificationChimes");
  expect(setting.config.enabled).toBe(false);

  // Saving is applied to a copy, not written, so the suite's daemon stays silent.
  let saved = setting.config;
  await page.route("**/api/v1/platform/notifications/chimes", async (route) => {
    if (route.request().method() === "POST")
      saved = { ...saved, ...route.request().postDataJSON() };
    await route.fulfill({ json: { kind: "NotificationChimes", config: saved } });
  });
  await stubAudio(page);
  await openApp(page, "/settings/sound");
  const toggle = page.getByRole("switch", { name: "Sound" });
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  expect(saved.enabled).toBe(true);

  await page.getByRole("button", { name: "Play the send sound" }).click();
  await expect.poll(() => audioStarts(page)).toBeGreaterThan(0);

  await pressSequence(page, "g", "i");
  await expect(mailRows(page).first()).toBeVisible();
  await page.getByTestId("mailbox-list").focus();
  const before = await audioStarts(page);
  await page.keyboard.press("j");
  await page.keyboard.press("k");
  await page.waitForTimeout(300);
  expect(await audioStarts(page)).toBe(before);

  await page.keyboard.press("e");
  await expect(page.getByText(/^Archived \d+ messages?$/)).toBeVisible();
  await expect.poll(() => audioStarts(page)).toBeGreaterThan(before);
  await page.keyboard.press("u");
  await expect(page.getByText(/^Undone$/)).toBeVisible();
});

/** Archive the first row with its hover button, then undo with the toast's button. */
async function pointerArchiveAndUndo(page: Page): Promise<void> {
  const row = mailRows(page).first();
  await row.hover();
  await row.locator('[title="Archive (e)"]').click();
  const toast = page
    .locator("[data-sonner-toast]")
    .filter({ hasText: /^Archived/ })
    .last();
  await toast.getByRole("button", { name: "Undo" }).click();
  await expect(page.getByText(/^Undone$/).first()).toBeVisible();
}

test("three pointer archives earn one key hint, and it never comes back", async ({ page }) => {
  await openList(page, "/m/inbox");
  const hint = page.getByText("Tip: press e for Archive.");
  await pointerArchiveAndUndo(page);
  await pointerArchiveAndUndo(page);
  await expect(hint).toHaveCount(0);
  await pointerArchiveAndUndo(page);
  await expect(hint).toBeVisible();

  await page.reload();
  await expect(mailRows(page).first()).toBeVisible();
  for (let use = 0; use < 3; use += 1) await pointerArchiveAndUndo(page);
  await expect(hint).toHaveCount(0);
});

test.describe("touch", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

  /** A real touch drag through the browser's input pipeline. */
  async function swipe(page: Page, x: number, y: number, dx: number, pause = false) {
    const cdp = await page.context().newCDPSession(page);
    const point = (px: number) => [{ x: px, y }];
    await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: point(x) });
    const steps = 12;
    for (let step = 1; step <= steps; step += 1) {
      await cdp.send("Input.dispatchTouchEvent", {
        type: "touchMove",
        touchPoints: point(x + (dx * step) / steps),
      });
      await page.waitForTimeout(16);
    }
    if (pause)
      return async () =>
        cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    return async () => undefined;
  }

  test("a swipe right archives the row, colour shows it first, and undo brings it back", async ({
    page,
  }) => {
    await openList(page, "/m/inbox");
    const row = mailRows(page).first();
    const id = await row.getAttribute("id");
    const box = (await row.boundingBox())!;
    const y = box.y + box.height / 2;

    const release = await swipe(page, box.x + 40, y, 130, true);
    const layer = page.getByTestId("swipe-layer");
    await expect(layer).toContainText("Archive");
    await expect(layer.locator("[data-armed]")).toHaveCount(1);
    await release();

    await expect(page.locator(`[id="${id}"]`)).toHaveCount(0);
    const toast = page.locator("[data-sonner-toast]").filter({ hasText: /^Archived/ });
    await toast.getByRole("button", { name: "Undo" }).click();
    await expect(page.locator(`[id="${id}"]`)).toBeVisible();
  });

  test("a short drag does nothing", async ({ page }) => {
    await openList(page, "/m/inbox");
    const row = mailRows(page).first();
    const id = await row.getAttribute("id");
    const box = (await row.boundingBox())!;
    await swipe(page, box.x + 40, box.y + box.height / 2, 60);
    await page.waitForTimeout(400);
    await expect(page.locator(`[id="${id}"]`)).toBeVisible();
    await expect(page.getByTestId("swipe-layer")).toHaveCount(0);
  });
});

test("holding e archives exactly one conversation", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.getByTestId("mailbox-list").focus();
  const first = await mailRows(page).first().getAttribute("id");
  const second = await mailRows(page).nth(1).getAttribute("id");
  // Playwright marks every down of a key that is already down as a repeat.
  for (let press = 0; press < 8; press += 1) await page.keyboard.down("e");
  await page.keyboard.up("e");
  await expect(page.locator(`[id="${first}"]`)).toHaveCount(0);
  await expect(page.locator(`[id="${second}"]`)).toBeVisible();
  await expect(page.locator("[data-sonner-toast]").filter({ hasText: /^Archived/ })).toHaveCount(1);
  await page.keyboard.press("u");
  await expect(page.locator(`[id="${first}"]`)).toBeVisible();
});

test("clearing the reply queue with Done earns the small low tide once", async ({ page }) => {
  let queued = true;
  const message = {
    id: "e2e-reply-later",
    thread_id: "e2e-reply-later-thread",
    subject: "Notes on the pricing page",
    snippet: "Could you look before Friday?",
    date: new Date().toISOString(),
    from: { name: "Maya Chen", email: "maya@example.com" },
  };
  await page.route("**/api/v1/mail/reply-later", (route) =>
    route.fulfill({ json: { kind: "ReplyQueue", messages: queued ? [message] : [] } }),
  );
  await page.route("**/api/v1/mail/reply-later/*", async (route) => {
    queued = (route.request().postDataJSON() as { flag: boolean }).flag;
    await route.fulfill({ json: { kind: "Ack" } });
  });
  await openList(page, "/reply-queue");
  await expect(mailRows(page)).toHaveCount(1);
  await page.getByTestId("mailbox-list").focus();
  await page.keyboard.press("w");
  const moment = page.getByTestId("low-tide");
  await expect(moment).toBeVisible();
  await expect(moment).toContainText("Low tide. Nobody's waiting on a reply.");

  await page.reload();
  await expect(page.getByText("Nothing waiting on a reply")).toBeVisible();
  await expect(page.getByTestId("low-tide")).toHaveCount(0);
});
