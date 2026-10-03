import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface GuideAnswer {
  guides: { mode: string; card_seen: boolean; header: string; never_had_any: string }[];
}

async function guide(page: Page) {
  return (await bridge<GuideAnswer>(page, "/api/v1/mail/modes/guide?mode=todo")).guides[0]!;
}

/** Bring To do's card back, as `mxr modes card todo --show` does. */
async function showCardAgain(page: Page) {
  await bridge(page, "/api/v1/mail/modes/todo/card", { seen: false });
}

async function waitForRows(page: Page) {
  await expect
    .poll(
      async () => {
        const runway = await bridge<{ runway: { now: unknown[] } }>(page, "/api/v1/mail/todos");
        return runway.runway.now.length;
      },
      { timeout: 60_000 },
    )
    .toBeGreaterThan(0);
}

test("the card shows once: Esc retires it here, after a reload and for the TUI", async ({
  page,
}) => {
  await waitForRows(page);
  await showCardAgain(page);
  await openApp(page, "/todo");
  await expect(page.getByTestId("mode-header")).toHaveText(
    "Things email asked you to do, ordered by when to act.",
  );
  const card = page.getByTestId("mode-card");
  await expect(card).toBeVisible();
  await expect(card).toContainText("Enter do it");
  await expect(card).toContainText("e tick off");
  await page.keyboard.press("Escape");
  await expect(card).toHaveCount(0);
  // No next card: this is not a tour.
  await expect(page.getByTestId("mode-card")).toHaveCount(0);
  await page.reload();
  await expect(page.getByTestId("todo-row").first()).toBeVisible();
  await expect(page.getByTestId("mode-card")).toHaveCount(0);
  // The daemon holds it, so every other client sees it closed too.
  expect((await guide(page)).card_seen).toBe(true);
});

test("using the mode's main verb retires the card", async ({ page }) => {
  await waitForRows(page);
  await showCardAgain(page);
  await openApp(page, "/todo");
  await expect(page.getByTestId("mode-card")).toBeVisible();
  await page.getByTestId("todo-action").first().click();
  await expect(page.getByTestId("mode-card")).toHaveCount(0);
  await expect.poll(async () => (await guide(page)).card_seen).toBe(true);
});

test("? leads with the mode's job, then the keys", async ({ page }) => {
  await waitForRows(page);
  await openApp(page, "/todo");
  await expect(page.getByTestId("todo-row").first()).toBeVisible();
  await page.keyboard.press("?");
  const help = page.getByRole("dialog", { name: "Keyboard" });
  await expect(help).toBeVisible();
  const modeHelp = help.getByTestId("mode-help");
  await expect(modeHelp).toContainText("To do: Things email asked you to do");
  await expect(modeHelp).toContainText("promises you made");
  await expect(modeHelp).toContainText("Z schedule");
});

/** The runway itself, not the catch-up or any other To do route. */
const RUNWAY_ROUTE = /\/api\/v1\/mail\/todos(\?.*)?$/;

const EMPTY_RUNWAY = {
  generated_at: "2026-10-05T09:00:00Z",
  header: "Things email asked you to do, ordered by when to act.",
  headline: "",
  now: [],
  coming_up: [],
  later: [],
  whenever: [],
  done_this_week: [],
  catchup_count: 0,
  expired_since_last_looked: 0,
  first_run: { complete: true, scanned: 0 },
};

test("both empty states teach: never had any, and clear for now", async ({ page }) => {
  const never = (await guide(page)).never_had_any;
  await page.route(RUNWAY_ROUTE, (route) =>
    route.fulfill({
      json: { kind: "TodoRunway", runway: { ...EMPTY_RUNWAY, empty_state: never } },
    }),
  );
  await openApp(page, "/todo");
  await expect(page.getByText(never, { exact: false })).toBeVisible();
  await expect(page.getByText("Press t on any email to add one yourself.")).toBeVisible();
  await expect(page.getByTestId("mode-card")).toHaveCount(0);

  await page.unrouteAll({ behavior: "ignoreErrors" });
  const clear = "Nothing needs you. Next: renew car insurance shows up Mon 19 Oct.";
  const renew = {
    id: "todo_renew",
    account_id: "acct",
    kind: "renewal",
    verb: "renew",
    title: "Renew car insurance",
    state: "open",
    origin: "rule",
    why: 'Here because: "renews on 26 October" (rule).',
    when_label: "shows up Mon 19 Oct · act by Mon 26 Oct",
    overdue: false,
    fields: [],
    user_touched: false,
    created_at: "2026-10-02T09:00:00Z",
    updated_at: "2026-10-02T09:00:00Z",
  };
  const runway = {
    ...EMPTY_RUNWAY,
    empty_state: clear,
    coming_up: [{ week_start: "2026-10-19", label: "wk of 19 Oct", todos: [renew] }],
  };
  await page.route(RUNWAY_ROUTE, (route) =>
    route.fulfill({ json: { kind: "TodoRunway", runway } }),
  );
  // A fresh load: the same URL again would only change the hash.
  await page.reload();
  await expect(page.getByTestId("todo-empty")).toHaveText(clear);
  await expect(page.getByTestId("coming-week")).toContainText("Renew car insurance");
  await page.unrouteAll({ behavior: "ignoreErrors" });
});
