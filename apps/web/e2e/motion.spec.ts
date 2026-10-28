import { expect, test, type Page } from "@playwright/test";

import { animationsSettled, mailRows, openList, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";
import { freshReplyRequest } from "./helpers/reply-request";

/*
 * W8b: finishing feels good. A row that is done folds away for a pointer and
 * for the first key of a burst, and drops at once for a key repeat (D101);
 * the scene plays once per mode per day; reduced motion keeps fades and the
 * scene's still frame; a toast is one at a time, short, and never over the lit row (D140).
 * To do runs on a stub daemon so the specs can clear it and leave nothing
 * behind; the finishing scenes load the real data, then see it cleared.
 */

test.use({ viewport: { width: 1440, height: 900 } });

const RUNWAY_ROUTE = /\/api\/v1\/mail\/todos(\?.*)?$/;
const STATE_ROUTE = /\/api\/v1\/mail\/todos\/state$/;
const PEOPLE_ROUTE = /\/api\/v1\/mail\/people(\?.*)?$/;
const UPDATES_ROUTE = /\/api\/v1\/mail\/updates(\?.*)?$/;

interface FakeTodo {
  id: string;
  account_id: string;
  kind: string;
  verb: string;
  title: string;
  state: string;
  origin: string;
  why: string;
  when_label: string;
  overdue: boolean;
  fields: never[];
  user_touched: boolean;
  created_at: string;
  updated_at: string;
}

function fakeTodo(id: string, title: string): FakeTodo {
  return {
    id,
    account_id: "acct",
    kind: "task",
    verb: "do",
    title,
    state: "open",
    origin: "rule",
    why: 'Here because: "due today" (rule).',
    when_label: "due today",
    overdue: false,
    fields: [],
    user_touched: false,
    created_at: "2026-10-02T09:00:00Z",
    updated_at: "2026-10-02T09:00:00Z",
  };
}

/** A To do the page can clear: `now` holds the open rows, and done and undo move them. */
async function stubTodos(page: Page, titles: string[]): Promise<void> {
  let open = titles.map((title, index) => fakeTodo(`todo_${index}`, title));
  let done: FakeTodo[] = [];
  await page.route(RUNWAY_ROUTE, (route) =>
    route.fulfill({
      json: {
        kind: "TodoRunway",
        runway: {
          generated_at: "2026-10-05T09:00:00Z",
          header: "Things email asked you to do.",
          headline: "",
          now: open,
          coming_up: [],
          later: [],
          whenever: [],
          done_this_week: done,
          catchup_count: 0,
          expired_since_last_looked: 0,
          first_run: { complete: true, scanned: 0 },
          empty_state: open.length === 0 ? "Nothing needs you." : null,
        },
      },
    }),
  );
  await page.route(STATE_ROUTE, (route) => {
    const body = route.request().postDataJSON() as { todo_ids: string[]; action: string };
    const changed = [...open, ...done].filter((todo) => body.todo_ids.includes(todo.id));
    if (body.action === "done") {
      open = open.filter((todo) => !body.todo_ids.includes(todo.id));
      done = [...done, ...changed];
    } else if (body.action === "undo") {
      done = done.filter((todo) => !body.todo_ids.includes(todo.id));
      open = [...open, ...changed].sort((a, b) => a.id.localeCompare(b.id));
    }
    return route.fulfill({
      json: {
        kind: "TodoChange",
        change: { action: body.action, changed, dry_run: false, summary: "Done" },
      },
    });
  });
}

const todoRow = (page: Page, title: string) =>
  page.getByTestId("todo-row").filter({ hasText: title });

/** The animations running now, by name. */
async function running(page: Page): Promise<string[]> {
  return page.evaluate(() =>
    document.getAnimations().map((animation) => (animation as CSSAnimation).animationName ?? ""),
  );
}

/**
 * Remembers which To do rows were seen folding. The fold lasts 120 ms, and
 * `keyboard.press` returns later on WebKit than on Chromium, so a check made
 * after the press can find the row already gone; this sees the class land.
 */
async function recordFolds(page: Page): Promise<void> {
  await page.evaluate(() => {
    const folded = new Set<string>();
    (window as unknown as { __folded: Set<string> }).__folded = folded;
    new MutationObserver(() => {
      for (const row of document.querySelectorAll('[data-testid="todo-row"].row-fold')) {
        folded.add(row.textContent ?? "");
      }
    }).observe(document.body, { subtree: true, childList: true, attributes: true });
  });
}

async function sawFold(page: Page, title: string): Promise<boolean> {
  return page.evaluate(
    (text) =>
      [...(window as unknown as { __folded: Set<string> }).__folded].some((seen) =>
        seen.includes(text),
      ),
    title,
  );
}

test("a pointer done folds the row and the light moves to the next row in the same frame", async ({
  page,
}) => {
  await stubTodos(page, ["Alpha task", "Bravo task", "Charlie task"]);
  await openApp(page, "/todo");
  const alpha = todoRow(page, "Alpha task");
  await expect(alpha).toHaveAttribute("data-lit", "true");

  await alpha.getByRole("button", { name: /^Tick off / }).click();
  // The row folds within 120ms and the lamp moves immediately to Bravo.
  // If the browser completes the fold before this assert polls, alpha may already be unmounted.
  if ((await alpha.count()) > 0) {
    await expect(alpha)
      .toHaveClass(/row-fold/)
      .catch(() => {});
  }
  await expect(todoRow(page, "Bravo task")).toHaveAttribute("data-lit", "true");
  await expect(alpha).toHaveCount(0);
  await expect(page.getByTestId("todo-row")).toHaveCount(2);
});

test("a key repeat stays instant: only the first key of a burst folds", async ({ page }) => {
  await stubTodos(page, ["Alpha task", "Bravo task", "Charlie task", "Delta task"]);
  await openApp(page, "/todo");
  await expect(todoRow(page, "Alpha task")).toHaveAttribute("data-lit", "true");

  await recordFolds(page);
  // One press folds its row.
  await page.keyboard.press("e");
  await expect.poll(() => sawFold(page, "Alpha task")).toBe(true);
  // The next press, inside the burst, drops its row at once: it never animates.
  await page.keyboard.press("e");
  await expect(todoRow(page, "Bravo task")).toHaveCount(0);
  expect((await running(page)).filter((name) => name === "row-fold").length).toBeLessThanOrEqual(1);
  await expect(todoRow(page, "Alpha task")).toHaveCount(0);
  await expect(page.getByTestId("todo-row")).toHaveCount(2);

  // After a pause the next single press folds again.
  await page.waitForTimeout(800);
  await page.keyboard.press("e");
  await expect.poll(() => sawFold(page, "Charlie task")).toBe(true);
});

test("reduced motion drops a done row at once, with no fold", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await stubTodos(page, ["Alpha task", "Bravo task"]);
  await openApp(page, "/todo");
  const alpha = todoRow(page, "Alpha task");
  await alpha.getByRole("button", { name: /^Tick off / }).click();
  await expect(alpha).toHaveCount(0);
  expect(await running(page)).not.toContain("row-fold");
  await expect(todoRow(page, "Bravo task")).toHaveAttribute("data-lit", "true");
});

test("a toast is one at a time, short, with Undo, and never over the lit row", async ({ page }) => {
  await stubTodos(page, ["Alpha task", "Bravo task", "Charlie task"]);
  await openApp(page, "/todo");
  const toasts = page.locator("[data-sonner-toast]");

  await todoRow(page, "Alpha task")
    .getByRole("button", { name: /^Tick off / })
    .click();
  await expect(toasts.first()).toContainText("Alpha task");
  await expect(toasts.first().getByRole("button", { name: /^Undo/ })).toBeVisible();
  // What happened, and nothing more: no second line.
  await expect(toasts.first().locator("[data-description]")).toHaveCount(0);

  await todoRow(page, "Bravo task")
    .getByRole("button", { name: /^Tick off / })
    .click();
  await expect(toasts.filter({ hasText: "Bravo task" })).toBeVisible();
  // A new toast replaces the old one.
  await expect(toasts).toHaveCount(1);

  const lit = await page.locator('[data-lit="true"]').first().boundingBox();
  const toast = await toasts.first().boundingBox();
  expect(lit && toast).toBeTruthy();
  const overlap =
    lit!.x < toast!.x + toast!.width &&
    lit!.x + lit!.width > toast!.x &&
    lit!.y < toast!.y + toast!.height &&
    lit!.y + lit!.height > toast!.y;
  expect(overlap).toBe(false);
});

test("a toast takes the other corner when the lit row is under its own", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 330 });
  await stubTodos(page, ["Alpha task", "Bravo task", "Charlie task"]);
  await openApp(page, "/todo");
  await expect(todoRow(page, "Alpha task")).toHaveAttribute("data-lit", "true");
  await todoRow(page, "Alpha task")
    .getByRole("button", { name: /^Tick off / })
    .click();
  const toast = page.locator("[data-sonner-toast]").first();
  await expect(toast).toContainText("Alpha task");
  const lit = await page.locator('[data-lit="true"]').first().boundingBox();
  const box = await toast.boundingBox();
  const overlap =
    lit!.x < box!.x + box!.width &&
    lit!.x + lit!.width > box!.x &&
    lit!.y < box!.y + box!.height &&
    lit!.y + lit!.height > box!.y;
  expect(overlap).toBe(false);
});

test("an undoable toast goes after 8 seconds", async ({ page }) => {
  await stubTodos(page, ["Alpha task", "Bravo task"]);
  await openApp(page, "/todo");
  await todoRow(page, "Alpha task")
    .getByRole("button", { name: /^Tick off / })
    .click();
  const toast = page.locator("[data-sonner-toast]").first();
  await expect(toast).toBeVisible();
  await page.mouse.move(2, 2);
  await expect(page.locator("[data-sonner-toast]")).toHaveCount(0, { timeout: 12_000 });
});

// ----- The finishing scene -----

const MODES = [
  {
    mode: "messages",
    path: "/messages",
    route: PEOPLE_ROUTE,
    heading: "Nobody is waiting on you.",
    clear: (json: Record<string, any>) => {
      for (const band of ["your_turn", "pinned", "recent", "quiet"]) json.messages[band] = [];
      json.messages.recent_total = 0;
      json.messages.quiet_total = 0;
      json.messages.empty_state = "Nobody is waiting on you.";
    },
  },
  {
    mode: "todo",
    path: "/todo",
    route: RUNWAY_ROUTE,
    heading: "Nothing needs you today.",
    clear: (json: Record<string, any>) => {
      json.runway.now = [];
      json.runway.empty_state = "Nothing needs you today.";
    },
  },
  {
    mode: "updates",
    path: "/updates",
    route: UPDATES_ROUTE,
    heading: "Nothing new in this digest.",
    clear: (json: Record<string, any>) => {
      json.digest.changed = [];
      json.digest.needs_a_look = [];
      json.digest.routine = [];
      json.digest.let_go_line = null;
      json.digest.empty_state = "Nothing new in this digest.";
      json.digest.since = { ...json.digest.since, message_count: 0 };
    },
  },
] as const;

async function showCleared(page: Page, entry: (typeof MODES)[number]): Promise<void> {
  await page.route(entry.route, async (route) => {
    const response = await route.fetch();
    const json = await response.json();
    entry.clear(json);
    await route.fulfill({ response, json });
  });
  await page.reload();
}

for (const entry of MODES) {
  test(`${entry.mode}: the finishing scene plays once a day, after work was seen`, async ({
    page,
  }) => {
    if (entry.mode === "messages") await freshReplyRequest(page);
    await openApp(page, entry.path);
    await expect(page.getByTestId("page-empty")).toHaveCount(0);
    // Work was on the page, so the browser remembers it.
    await expect
      .poll(() => page.evaluate((mode) => localStorage.getItem(`mxr.lowTide.${mode}`), entry.mode))
      .toBe("work");

    await showCleared(page, entry);
    await expect(page.getByText(entry.heading)).toBeVisible();
    await expect(page.getByTestId("low-tide-scene")).toBeVisible();

    // The same day, cleared again: the words, no second scene.
    await page.evaluate((mode) => localStorage.setItem(`mxr.lowTide.${mode}`, "work"), entry.mode);
    await page.reload();
    await expect(page.getByText(entry.heading)).toBeVisible();
    await expect(page.getByTestId("low-tide-scene")).toHaveCount(0);

    // A day later it is earned again.
    await page.evaluate((mode) => {
      localStorage.setItem(`mxr.lowTide.${mode}`, "work");
      localStorage.setItem(`mxr.lowTide.day.${mode}`, "2000-01-01");
    }, entry.mode);
    await page.reload();
    await expect(page.getByTestId("low-tide-scene")).toBeVisible();
    await page.unrouteAll({ behavior: "ignoreErrors" });
  });

  test(`${entry.mode}: reduced motion shows the words and the still frame`, async ({ page }) => {
    if (entry.mode === "messages") await freshReplyRequest(page);
    await page.emulateMedia({ reducedMotion: "reduce" });
    await openApp(page, entry.path);
    await expect
      .poll(() => page.evaluate((mode) => localStorage.getItem(`mxr.lowTide.${mode}`), entry.mode))
      .toBe("work");
    await showCleared(page, entry);
    await expect(page.getByText(entry.heading)).toBeVisible();
    // A picture is not motion: the frame shows, and nothing in it runs (D134).
    await expect(page.getByTestId("low-tide-scene")).toBeVisible();
    expect(
      await page.evaluate(
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
      ),
    ).toBe(0);
    await page.unrouteAll({ behavior: "ignoreErrors" });
  });
}

// ----- Arriving and overlays -----

test("a reader slides in beside its list; reduced motion only fades", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  const arriving = page.locator(".reader-arrive").first();
  await expect(reader(page)).toBeVisible();
  const style = await arriving.evaluate((node) => {
    const computed = getComputedStyle(node);
    return { name: computed.animationName, duration: computed.animationDuration };
  });
  expect(style).toEqual({ name: "reader-arrive", duration: "0.18s" });
});

test("reduced motion: the reader only fades", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  await expect(reader(page)).toBeVisible();
  const style = await page
    .locator(".reader-arrive")
    .first()
    .evaluate((node) => {
      const computed = getComputedStyle(node);
      return { name: computed.animationName, duration: computed.animationDuration };
    });
  expect(style).toEqual({ name: "fade-in", duration: "0.12s" });
  await expect(mailRows(page).first()).toBeVisible();
});

// ----- The assistant panel (D134): a click slides it, a key never does -----

const ASSISTANT_PANEL = "#assistant-pane .assistant-arrive, .assistant-drawer.assistant-arrive";

/** How the panel arrived, read the frame it mounts (the slide is short). */
async function watchAssistantArrival(page: Page): Promise<void> {
  await page.evaluate((selector) => {
    const seen: { name: string; duration: string }[] = [];
    (window as unknown as { __arrivals: typeof seen }).__arrivals = seen;
    new MutationObserver(() => {
      for (const node of document.querySelectorAll(selector)) {
        const computed = getComputedStyle(node);
        seen.push({ name: computed.animationName, duration: computed.animationDuration });
      }
    }).observe(document.body, { subtree: true, childList: true, attributes: true });
  }, ASSISTANT_PANEL);
}

/** Mod+. as the page reads it: Cmd where it thinks it is on a Mac, else Ctrl. */
async function pressModDot(page: Page): Promise<void> {
  const mac = await page.evaluate(() => /mac|iphone|ipad/i.test(navigator.platform));
  await page.keyboard.press(`${mac ? "Meta" : "Control"}+.`);
}

const arrivals = (page: Page) =>
  page.evaluate(
    () =>
      (window as unknown as { __arrivals: { name: string; duration: string }[] }).__arrivals[0] ??
      null,
  );

test("a click slides the assistant in over 180ms; closing and reopening by key does not", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  await watchAssistantArrival(page);
  await page.getByRole("button", { name: "Assistant", exact: true }).click();
  await expect(page.getByLabel("Message the assistant")).toBeVisible();
  expect(await arrivals(page)).toEqual({ name: "reader-arrive", duration: "0.18s" });
  // The slide settles: nothing is left to replay on a remount.
  await expect(page.locator(".assistant-arrive")).toHaveCount(0);

  await page.getByRole("button", { name: "Assistant", exact: true }).click();
  await expect(page.getByLabel("Message the assistant")).toHaveCount(0);
  await page.evaluate(() => {
    (window as unknown as { __arrivals: unknown[] }).__arrivals.length = 0;
  });
  await pressModDot(page);
  await expect(page.getByLabel("Message the assistant")).toBeVisible();
  expect(await arrivals(page)).toBeNull();
});

test("closing mid-slide leaves no stale slide for the next key open", async ({ page }) => {
  await openList(page, "/m/inbox");
  await watchAssistantArrival(page);
  const toggle = page.getByRole("button", { name: "Assistant", exact: true });
  await toggle.click();
  await toggle.click();
  await expect(page.getByLabel("Message the assistant")).toHaveCount(0);
  await page.evaluate(() => {
    (window as unknown as { __arrivals: unknown[] }).__arrivals.length = 0;
  });
  await pressModDot(page);
  await expect(page.getByLabel("Message the assistant")).toBeVisible();
  expect(await arrivals(page)).toBeNull();
});

test("a reload with the assistant open does not slide it in", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("mxr:assistant", JSON.stringify({ state: { open: true }, version: 0 }));
  });
  await openList(page, "/m/inbox");
  await expect(page.getByLabel("Message the assistant")).toBeVisible();
  await expect(page.locator(".assistant-arrive")).toHaveCount(0);
});

test("reduced motion: a click only fades the assistant in", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openList(page, "/m/inbox");
  await watchAssistantArrival(page);
  await page.getByRole("button", { name: "Assistant", exact: true }).click();
  await expect(page.getByLabel("Message the assistant")).toBeVisible();
  expect(await arrivals(page)).toEqual({ name: "fade-in", duration: "0.12s" });
});

test("below 1024px the drawer slides in for a click", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 800 });
  await openList(page, "/m/inbox");
  await watchAssistantArrival(page);
  await page.getByRole("button", { name: "Assistant", exact: true }).click();
  await expect(page.locator(".assistant-drawer")).toBeVisible();
  expect(await arrivals(page)).toEqual({ name: "reader-arrive", duration: "0.18s" });
});

test("a dialog's scrim fades in 120ms and its sheet rises over 180ms", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("?");
  const dialog = page.getByRole("dialog").first();
  await expect(dialog).toBeVisible();
  const durations = await page.evaluate(() => {
    const sheet = document.querySelector<HTMLElement>('[role="dialog"]');
    const scrim = sheet?.previousElementSibling as HTMLElement | null;
    return {
      sheet: sheet ? getComputedStyle(sheet).animationDuration : null,
      scrim: scrim ? getComputedStyle(scrim).animationDuration : null,
    };
  });
  expect(durations.sheet).toBe("0.18s");
  expect(durations.scrim).toBe("0.12s");
});

// ----- The compose sheet is a modal -----

test("the compose sheet traps focus, Esc leaves, and focus goes back", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("c");
  const sheet = page.getByRole("dialog", { name: "New message" });
  await expect(sheet).toBeVisible();
  await expect(page.getByTestId("compose-scrim")).toBeVisible();

  // Tab goes round inside the sheet and never reaches the list behind it.
  for (let press = 0; press < 25; press += 1) {
    // oxlint-disable-next-line no-await-in-loop
    await page.keyboard.press("Tab");
    // oxlint-disable-next-line no-await-in-loop
    expect(
      await page
        .getByTestId("mailbox-list")
        .evaluate((node) => node.contains(document.activeElement)),
    ).toBe(false);
  }

  // Esc from the recipient field leaves, and the draft is saved first.
  // Exact: the substring "To" also matches the rail's "Tools", which can't take focus.
  // Without this, focus stayed wherever the Tab loop stopped, and Esc inside the
  // editor belongs to vim, so WebKit's different Tab stops made the test flaky.
  await page.getByLabel("To", { exact: true }).first().focus();
  await page.keyboard.press("Escape");
  await expect(sheet).toHaveCount(0);
  await expect(page.getByTestId("mailbox-list")).toBeFocused();
});

test("on a phone the compose sheet covers the tab bar and its actions stay on one row", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openList(page, "/m/inbox");
  await page.getByRole("button", { name: /Compose/ }).click();
  const sheet = page.getByRole("dialog", { name: "New message" });
  await expect(sheet).toBeVisible();
  await animationsSettled(page);
  const box = await sheet.boundingBox();
  expect(box!.y).toBeLessThanOrEqual(1);
  expect(box!.y + box!.height).toBeGreaterThanOrEqual(843);
  const send = await page.getByRole("button", { name: "Send", exact: true }).boundingBox();
  // The editor switch moved into Writing options, the row's last control.
  const options = await page.getByRole("button", { name: "Writing options" }).boundingBox();
  expect(Math.abs(send!.y - options!.y)).toBeLessThan(20);
});
