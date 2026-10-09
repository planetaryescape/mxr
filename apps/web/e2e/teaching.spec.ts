import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface GuideAnswer {
  guides: {
    mode: string;
    header: string;
    never_had_any: string;
    hints: { id: string; text: string; seen: boolean }[];
  }[];
}

async function guide(page: Page) {
  return (await bridge<GuideAnswer>(page, "/api/v1/mail/modes/guide?mode=todo")).guides[0]!;
}

/** Whether the daemon holds a hint as dismissed: what every client reads. */
async function hintSeen(page: Page, id: string): Promise<boolean> {
  const { guides } = await bridge<GuideAnswer>(page, "/api/v1/mail/modes/guide");
  const hint = guides.flatMap((entry) => entry.hints).find((entry) => entry.id === id);
  expect(hint, `the daemon knows ${id}`).toBeTruthy();
  return hint!.seen;
}

/** Bring a hint back, as `mxr modes hint ID --show` does. */
async function showHint(page: Page, id: string) {
  await bridge(page, `/api/v1/mail/hints/${encodeURIComponent(id)}`, { seen: false });
}

/** e2e-server dismisses every hint so other specs never meet one; so do we. */
async function dismissEveryHint(page: Page) {
  const { guides } = await bridge<GuideAnswer>(page, "/api/v1/mail/modes/guide");
  for (const id of new Set(guides.flatMap((entry) => entry.hints.map((hint) => hint.id)))) {
    await bridge(page, `/api/v1/mail/hints/${encodeURIComponent(id)}`, { seen: true });
  }
}

test.afterEach(async ({ page }) => {
  await dismissEveryHint(page);
});

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

const hints = (page: Page) => page.getByTestId("hint");
/** The first To do row with a runway bar: where the runway hint anchors. */
const firstBarRow = (page: Page) =>
  page
    .getByTestId("todo-row")
    .filter({ has: page.getByTestId("runway-bar") })
    .first();

test("no page-top card, and no hint on arrival before the user does anything", async ({ page }) => {
  await waitForRows(page);
  await showHint(page, "todo.runway");
  await openApp(page, "/todo");
  await expect(page.getByTestId("mode-header")).toHaveText(
    "Things email asked you to do, ordered by when to act.",
  );
  await expect(page.getByTestId("todo-row").first()).toBeVisible();
  await expect(page.getByTestId("mode-card")).toHaveCount(0);
  // Arriving is not a need: give a stray hint every chance to appear.
  await page.waitForLoadState("networkidle");
  await expect(hints(page)).toHaveCount(0);
});

test("the runway hint shows at the first bar at first need; Esc dismisses it for good", async ({
  page,
}) => {
  await waitForRows(page);
  await showHint(page, "todo.runway");
  await openApp(page, "/todo");
  await expect(page.getByTestId("todo-row").first()).toBeVisible();
  await page.keyboard.press("j");
  const hint = firstBarRow(page).getByTestId("hint");
  await expect(hint).toBeVisible();
  await expect(hint).toContainText("The bar fills from when this showed up to when it's due");
  await expect(hint).toContainText("Enter");
  await expect(hints(page)).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(hints(page)).toHaveCount(0);
  // Never again: not on a reload, and not in the TUI or the CLI, which
  // read the same daemon state.
  await page.reload();
  await expect(page.getByTestId("todo-row").first()).toBeVisible();
  await page.keyboard.press("j");
  await expect(hints(page)).toHaveCount(0);
  expect(await hintSeen(page, "todo.runway")).toBe(true);
});

test("doing what the hint names dismisses it", async ({ page }) => {
  await waitForRows(page);
  await showHint(page, "todo.runway");
  await openApp(page, "/todo");
  await expect(page.getByTestId("todo-row").first()).toBeVisible();
  await page.keyboard.press("j");
  await expect(firstBarRow(page).getByTestId("hint")).toBeVisible();
  await firstBarRow(page).getByTestId("todo-title").click();
  await expect(firstBarRow(page)).toHaveAttribute("aria-current", "true");
  await page.keyboard.press("Enter");
  await expect.poll(() => hintSeen(page, "todo.runway")).toBe(true);
  await openApp(page, "/todo");
  await expect(page.getByTestId("todo-row").first()).toBeVisible();
  await page.keyboard.press("j");
  await expect(hints(page)).toHaveCount(0);
});

test("never more than one hint, and dismissing one doesn't bring up the next", async ({ page }) => {
  await waitForNow(page);
  await showHint(page, "now.from_mode");
  await showHint(page, "updates.let_go");
  await openApp(page, "/now");
  await expect(page.getByTestId("now-row").first()).toBeVisible();
  await expect(hints(page)).toHaveCount(0);
  await page.keyboard.press("j");
  const first = page.getByTestId("now-row").first().getByTestId("hint");
  await expect(first).toContainText("Each row comes from a mode; Enter opens it there.");
  await expect(hints(page)).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(hints(page)).toHaveCount(0);
  // The Updates card's hint waits for its own need, the cursor reaching it.
  await page.keyboard.press("k");
  await expect(hints(page)).toHaveCount(0);
  const updates = page.getByTestId("now-section-updates");
  if ((await updates.count()) > 0) {
    await updates.getByTestId("now-row").hover();
    await expect(updates.getByTestId("hint")).toContainText("A lets go of this digest only");
    await expect(hints(page)).toHaveCount(1);
  }
});

for (const colorScheme of ["dark", "light"] as const) {
  test(`axe passes with a hint showing (${colorScheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme });
    await waitForRows(page);
    await showHint(page, "todo.runway");
    await openApp(page, "/todo");
    await expect(page.getByTestId("todo-row").first()).toBeVisible();
    await page.keyboard.press("j");
    await expect(hints(page)).toHaveCount(1);
    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa"])
      .exclude("iframe")
      .analyze();
    const blocking = results.violations
      .filter((violation) => violation.impact === "serious" || violation.impact === "critical")
      .flatMap((violation) => violation.nodes.map((node) => `${violation.id}: ${node.target}`));
    expect(blocking).toEqual([]);
  });
}

async function waitForNow(page: Page) {
  await expect
    .poll(
      async () =>
        (await bridge<{ now: { item_count: number } }>(page, "/api/v1/mail/now")).now.item_count,
      { timeout: 60_000 },
    )
    .toBeGreaterThan(0);
}

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

// ----- Messages -----

const SAMIR = "person:samir@launchpad.example";

interface PeopleAnswer {
  messages: Record<
    "your_turn" | "recent" | "quiet",
    {
      id: string;
      kind: string;
      topics: { thread_id: string; subject: string; state: string }[];
    }[]
  >;
}

test("the topics hint sits under a person's topic list and ] dismisses it", async ({ page }) => {
  await showHint(page, "messages.topics");
  await openApp(page, `/messages?person=${encodeURIComponent(SAMIR)}`);
  const person = page.getByTestId("person-page");
  await expect(person.getByTestId("topic").nth(1)).toBeVisible();
  await expect(hints(page)).toHaveCount(0);
  // Any click on the page is a use; the topic list is already its need.
  await person.getByTestId("person-name").click();
  const hint = person.getByRole("navigation", { name: "Topics" }).getByTestId("hint");
  await expect(hint).toContainText("Every conversation with this person, yours to answer first");
  await page.keyboard.press("]");
  await expect(hints(page)).toHaveCount(0);
  await expect.poll(() => hintSeen(page, "messages.topics")).toBe(true);
});

test("Got it's hint shows when Got it is about to be used", async ({ page }) => {
  await showHint(page, "messages.got_it");
  await openApp(page, `/messages?person=${encodeURIComponent(SAMIR)}`);
  const person = page.getByTestId("person-page");
  await expect(person.getByTestId("got-it")).toBeVisible();
  await person.getByTestId("person-name").click();
  await expect(hints(page)).toHaveCount(0);
  await person.getByTestId("got-it").hover();
  await expect(person.getByTestId("hint")).toContainText("Got it (.) sends a short note");
  await person.getByRole("button", { name: "Dismiss hint (Esc)" }).click();
  await expect(hints(page)).toHaveCount(0);
  expect(await hintSeen(page, "messages.got_it")).toBe(true);
});

test("the first done here carries its hint in the toast, and only the first", async ({ page }) => {
  await showHint(page, "done_here");
  const people = (await bridge<PeopleAnswer>(page, "/api/v1/mail/people")).messages;
  // Demo people with a conversation still open in Messages: other specs
  // leave their own sends and dones behind.
  const open = (row: (typeof people.recent)[number]) =>
    row.topics.find((topic) => topic.state !== "done" && !topic.subject.startsWith("e2e-"));
  const candidates = people.recent.filter(
    (row) => row.kind === "person" && row.id !== SAMIR && open(row) !== undefined,
  );
  expect(candidates.length).toBeGreaterThan(1);
  const doneOn = async (row: (typeof candidates)[number]) => {
    const topic = open(row)!.thread_id;
    await openApp(page, `/messages?person=${encodeURIComponent(row.id)}&topic=${topic}`);
    await expect(page.getByTestId("conversation")).toBeVisible();
    await page.getByTestId("done-here").click();
  };
  const text = "Done here (e) only clears this mode; it stays in To do until done there.";
  await doneOn(candidates[0]!);
  await expect(page.getByText(text)).toBeVisible();
  await expect.poll(() => hintSeen(page, "done_here")).toBe(true);
  await doneOn(candidates[1]!);
  // The second done's handoff toast, whatever its words, without the hint.
  await expect(page.locator("[data-sonner-toast]").first()).toBeVisible();
  await expect(page.getByText(text)).toHaveCount(0);
});

test("? in Messages leads with the mode", async ({ page }) => {
  await openApp(page, "/messages");
  await expect(page.getByTestId("messages-row").first()).toBeVisible();
  await page.keyboard.press("?");
  const modeHelp = page.getByRole("dialog").getByTestId("mode-help");
  await expect(modeHelp).toContainText("Messages: People you talk with");
  await expect(modeHelp).toContainText(". got it");
});

const PEOPLE_ROUTE = /\/api\/v1\/mail\/people(\?.*)?$/;

test("Messages' empty states teach: never had any, and nobody waiting", async ({ page }) => {
  const empty = {
    generated_at: "2026-10-05T09:00:00Z",
    header: "People you talk with, one row each. Reply or mark done.",
    your_turn: [],
    pinned: [],
    recent: [],
    quiet: [],
    recent_total: 0,
    quiet_total: 0,
    row_count: 0,
    thread_count: 0,
    merge_suggestion_count: 0,
  };
  const never =
    "When someone writes to you and you've written to them, they show up here, one row per person, with what they asked you.";
  await page.route(PEOPLE_ROUTE, (route) =>
    route.fulfill({ json: { kind: "Messages", messages: { ...empty, empty_state: never } } }),
  );
  await openApp(page, "/messages");
  await expect(page.getByTestId("messages-empty")).toContainText(never);

  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.route(PEOPLE_ROUTE, (route) =>
    route.fulfill({
      json: {
        kind: "Messages",
        messages: {
          ...empty,
          empty_state: "Nobody is waiting on you.",
          lapsed: [
            {
              account_id: "acct",
              person: { id: "ari@fieldkit.example", name: "Ari Stone", addresses: [] },
              line: "Ari usually writes every week. Last: 19 days ago.",
            },
          ],
        },
      },
    }),
  );
  await page.reload();
  await expect(page.getByTestId("messages-empty")).toContainText("Nobody is waiting on you.");
  await expect(page.getByTestId("lapsed-line")).toHaveText(
    "Ari usually writes every week. Last: 19 days ago.",
  );
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

test("Updates' source hint sits under the first line after a key; Esc dismisses it", async ({
  page,
}) => {
  await showHint(page, "updates.source");
  await openApp(page, "/updates");
  const first = page.getByTestId("update-line").first();
  await expect(first).toBeVisible();
  await expect(hints(page)).toHaveCount(0);
  await page.keyboard.press("j");
  await expect(first.getByTestId("hint")).toContainText("e lets go of this source only");
  await expect(hints(page)).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(hints(page)).toHaveCount(0);
  await expect.poll(() => hintSeen(page, "updates.source")).toBe(true);
});
