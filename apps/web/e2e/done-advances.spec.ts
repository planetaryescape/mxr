/*
 * After a row leaves its list, the pane beside it moves on: done here in
 * Messages opens the person's next topic, else the next person; To do,
 * Updates and the Inbox reader open the next conversation (the previous
 * one at the end), an empty list empties the pane, and undo brings the row
 * back and opens it again. Every journey undoes what it did, through the
 * daemon, so the other specs see the demo as it was.
 */

import { expect, test, type Page, type Response } from "@playwright/test";

import { expectCursorOn, mailList, openList, reader } from "./helpers/mail";
import { bridge, bridgeAuth, openApp, readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface Topic {
  thread_id: string;
  subject: string;
  state: string;
}
interface Row {
  id: string;
  kind: string;
  title: string;
  band: string;
  topics: Topic[];
}
interface MessagesAnswer {
  messages: { your_turn: Row[]; pinned: Row[]; recent: Row[]; quiet: Row[] };
}

async function people(page: Page) {
  return (await bridge<MessagesAnswer>(page, "/api/v1/mail/people")).messages;
}

/** Each person's band, to compare before and after a test's undo. */
async function bandsByPerson(page: Page): Promise<Record<string, string>> {
  const data = await people(page);
  const rows = [...data.your_turn, ...data.pinned, ...data.recent, ...data.quiet];
  return Object.fromEntries(rows.map((row) => [row.id, row.band]));
}

const open = (row: Row) => row.topics.filter((topic) => topic.state !== "done");

async function waitForPeople(page: Page) {
  await expect
    .poll(async () => (await people(page)).your_turn.length, { timeout: 60_000 })
    .toBeGreaterThan(2);
}

/**
 * Every daemon mutation the page makes (done here, archive), so the test
 * can undo them all at the end. `u` reverses only the newest.
 */
interface Tracked {
  /** One per request, in the order the page sent them. */
  pending: Promise<string | null>[];
}

/** Mode done answers `{ mutation_id }`; mail mutations `{ result: { mutation_id } }`. */
interface MutationAnswer {
  mutation_id?: string | null;
  result?: { mutation_id?: string | null } | null;
}

const idOf = (answer: MutationAnswer) => answer.mutation_id ?? answer.result?.mutation_id ?? null;

async function mutationId(response: Response | null): Promise<string | null> {
  if (!response) return null;
  // SAFETY: both routes answer with a daemon mutation, shaped as above.
  const body = (await response.json()) as MutationAnswer;
  return idOf(body);
}

function trackMutations(page: Page): Tracked {
  const tracked: Tracked = { pending: [] };
  // From the request, so one still on its way when the test ends is waited for.
  page.on("request", (request) => {
    if (request.method() !== "POST") return;
    if (!/\/modes\/[a-z]+\/done$|\/mutations\/(archive|trash)$/.test(request.url())) return;
    tracked.pending.push(
      request
        .response()
        .then(mutationId)
        .catch(() => null),
    );
  });
  return tracked;
}

async function undoAll(page: Page, tracked: Tracked) {
  // In request order, not the order the answers came back: the daemon takes
  // overlapping requests in any order, and undoing in the wrong order leaves
  // an earlier state behind.
  const ids = (await Promise.all(tracked.pending)).filter((id): id is string => id !== null);
  for (const id of ids.toReversed()) {
    // One already undone from the page answers with an error; that's fine.
    // oxlint-disable-next-line no-await-in-loop
    await page.request
      .post(`${readE2EState().bridgeUrl}/api/v1/mail/mutations/undo`, {
        headers: bridgeAuth(),
        data: { mutation_id: id },
      })
      .catch(() => undefined);
  }
}

function personRow(page: Page, id: string) {
  return page.locator(`[data-testid="messages-row"][data-row-id="${id}"]`);
}

function toast(page: Page, text: RegExp) {
  return page.locator("[data-sonner-toast]").filter({ hasText: text });
}

const currentTopic = (page: Page) =>
  page.locator('[data-testid="person-page"] [data-testid="topic"][aria-current="true"]');

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

test.describe("Messages: done here moves on", () => {
  let mutations: Tracked;
  let bandsBefore: Record<string, string>;
  test.beforeEach(async ({ page }) => {
    mutations = trackMutations(page);
    bandsBefore = await bandsByPerson(page);
  });
  test.afterEach(async ({ page }) => {
    await undoAll(page, mutations);
    // These tests mark people done; later specs rely on everyone being back
    // where they were, so the test that leaves somebody behind is the one
    // that fails.
    await expect
      .poll(() => bandsByPerson(page), { timeout: 10_000, message: "everyone is back in their band" })
      .toEqual(bandsBefore);
  });

  test("a person with topics left stays open on the next one, and u reopens the done one", async ({
    page,
  }) => {
    await waitForPeople(page);
    const data = await people(page);
    const person = data.your_turn.find(
      (row) =>
        row.kind === "person" && open(row).length > 1 && row.topics[0]?.state === "your_turn",
    );
    test.skip(!person, "no person whose turn it is with another topic open");
    const done = person!.topics[0]!;
    const next = open(person!).find((topic) => topic.thread_id !== done.thread_id)!;
    const count = open(person!).length;

    await openApp(
      page,
      `/messages?person=${encodeURIComponent(person!.id)}&topic=${done.thread_id}`,
    );
    await expect(page.getByTestId("conversation-subject")).toHaveText(done.subject);
    await expect(page.getByTestId("topics-left")).toHaveText(
      `${count} topics left with ${person!.title}`,
    );
    await expect(personRow(page, person!.id).getByTestId("row-topic-count")).toHaveText(
      `${count} topics`,
    );

    await page.keyboard.press("e");
    // The topic just done is never the one left open.
    await expect(currentTopic(page)).toContainText(next.subject);
    await expect(page.getByTestId("conversation-subject")).toHaveText(next.subject);
    // A "now showing" moment on the topic and the conversation header.
    await expect(currentTopic(page).getByTestId("arrival")).toHaveCount(1);
    await expect(page.getByTestId("conversation-subject").getByTestId("arrival")).toHaveCount(1);
    await expect(
      toast(page, new RegExp(`^Done: ${escape(done.subject)}\\. Next: ${escape(next.subject)}\\.`)),
    ).toBeVisible();
    // The person keeps their row, one topic fewer.
    await expect(page.getByTestId("topics-left")).toHaveText(
      count - 1 === 1
        ? `1 topic left with ${person!.title}`
        : `${count - 1} topics left with ${person!.title}`,
    );
    await expect(personRow(page, person!.id)).toBeVisible();
    if (count - 1 > 1) {
      await expect(personRow(page, person!.id).getByTestId("row-topic-count")).toHaveText(
        `${count - 1} topics`,
      );
    } else {
      await expect(personRow(page, person!.id).getByTestId("row-topic-count")).toHaveCount(0);
    }

    await page.keyboard.press("u");
    await expect(page.getByText("Undone").first()).toBeVisible();
    await expect(currentTopic(page)).toContainText(done.subject);
    await expect(page.getByTestId("topics-left")).toHaveText(
      `${count} topics left with ${person!.title}`,
    );
  });

  test("a person's last topic opens the next person, and u brings them back", async ({ page }) => {
    await waitForPeople(page);
    const data = await people(page);
    const rows = [...data.your_turn, ...data.recent];
    const at = rows.findIndex((row, index) => open(row).length === 1 && index < rows.length - 1);
    test.skip(at < 0, "no row with one topic left before the end");
    const person = rows[at]!;
    const next = rows[at + 1]!;
    const topic = open(person)[0]!;

    await openApp(
      page,
      `/messages?person=${encodeURIComponent(person.id)}&topic=${topic.thread_id}`,
    );
    await expect(page.getByTestId("conversation-subject")).toHaveText(topic.subject);
    await page.keyboard.press("e");

    await expect(personRow(page, next.id)).toHaveAttribute("aria-current", "true");
    await expect(personRow(page, next.id).getByTestId("arrival")).toHaveCount(1);
    await expect(page.getByTestId("person-name")).toHaveText(next.title);
    await expect(
      toast(
        page,
        new RegExp(`^Done with ${escape(person.title)}\\. Next: ${escape(next.title)}\\.`),
      ),
    ).toBeVisible();

    await page.keyboard.press("u");
    await expect(page.getByTestId("person-name")).toHaveText(person.title);
    await expect(currentTopic(page)).toContainText(topic.subject);
    await expect(personRow(page, person.id)).toHaveAttribute("aria-current", "true");
  });

  test("the last person goes to the one before, and the empty list empties the page", async ({
    page,
  }) => {
    await waitForPeople(page);
    const mine = (await people(page)).your_turn;
    const last = mine.at(-1)!;
    const before = mine.at(-2)!;
    await openApp(page, `/messages?turn=mine&person=${encodeURIComponent(last.id)}`);
    await expect(page.getByTestId("person-name")).toHaveText(last.title);
    // Done on each of their topics; the last one moves to the person before.
    for (let left = open(last).length; left > 0; left -= 1) {
      // oxlint-disable-next-line no-await-in-loop
      await expect(page.locator('[aria-busy="true"]')).toHaveCount(0);
      // oxlint-disable-next-line no-await-in-loop
      await expect(page.getByTestId("conversation-subject")).toBeVisible();
      // oxlint-disable-next-line no-await-in-loop
      await page.keyboard.press("e");
    }
    await expect(page.getByTestId("person-name")).toHaveText(before.title);
    await expect(personRow(page, before.id)).toHaveAttribute("aria-current", "true");

    // Then everyone, until nobody is left: the page shows the empty state.
    for (let presses = 0; presses < 60; presses += 1) {
      // oxlint-disable-next-line no-await-in-loop
      if (await page.getByTestId("messages-empty").isVisible()) break;
      // oxlint-disable-next-line no-await-in-loop
      await expect(page.locator('[aria-busy="true"]')).toHaveCount(0);
      // oxlint-disable-next-line no-await-in-loop
      if (!(await page.getByTestId("conversation-subject").isVisible())) continue;
      // oxlint-disable-next-line no-await-in-loop
      await page.keyboard.press("e");
      // oxlint-disable-next-line no-await-in-loop
      await page.waitForTimeout(150);
    }
    await expect(page.getByTestId("messages-empty")).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText("Nobody here yet")).toBeVisible();
    await expect(page.getByTestId("conversation-subject")).toHaveCount(0);
  });

  test("undo after moving on restores the person without pulling you back", async ({ page }) => {
    await waitForPeople(page);
    const data = await people(page);
    const rows = [...data.your_turn, ...data.recent];
    const at = rows.findIndex((row, index) => open(row).length === 1 && index < rows.length - 2);
    test.skip(at < 0, "no row with one topic left and two rows after it");
    const person = rows[at]!;
    const next = rows[at + 1]!;
    const topic = open(person)[0]!;
    await openApp(
      page,
      `/messages?person=${encodeURIComponent(person.id)}&topic=${topic.thread_id}`,
    );
    await expect(page.getByTestId("conversation-subject")).toHaveText(topic.subject);
    await page.keyboard.press("e");
    await expect(page.getByTestId("person-name")).toHaveText(next.title);
    // Somewhere else before the undo.
    await page.keyboard.press("j");
    await expect(personRow(page, next.id)).not.toHaveAttribute("aria-current", "true");
    const movedTo = await page.getByTestId("person-name").textContent();
    await page.keyboard.press("u");
    const restored = toast(page, new RegExp(`^Restored: ${escape(person.title)}`));
    await expect(restored).toBeVisible();
    await expect(page.getByTestId("person-name")).toHaveText(movedTo!);
    // Its Open goes back to the restored topic.
    await restored.getByRole("button", { name: "Open" }).click();
    await expect(page.getByTestId("person-name")).toHaveText(person.title);
    await expect(currentTopic(page)).toContainText(topic.subject);
  });

  test("a second e while the next topic loads sends one done for each topic", async ({ page }) => {
    await waitForPeople(page);
    const person = (await people(page)).your_turn.find((row) => open(row).length > 1);
    test.skip(!person, "no person with two topics open");
    const done = open(person!)[0]!;
    const threads: string[] = [];
    page.on("request", (request) => {
      if (/\/modes\/messages\/done$/.test(request.url())) {
        // SAFETY: the page's own markModeDone sends `{ thread_ids: string[] }` here.
        const body = request.postDataJSON() as { thread_ids: string[] };
        threads.push(...body.thread_ids);
      }
    });
    // Slow the next topic's page, so the second press lands while it loads.
    await page.route("**/api/v1/mail/people/page?*", async (route) => {
      if (route.request().url().includes(done.thread_id)) return route.continue();
      await new Promise((resolve) => setTimeout(resolve, 800));
      return route.continue();
    });
    await openApp(
      page,
      `/messages?person=${encodeURIComponent(person!.id)}&topic=${done.thread_id}`,
    );
    await expect(page.getByTestId("conversation-subject")).toHaveText(done.subject);
    await page.keyboard.press("e");
    await page.keyboard.press("e");
    await expect(toast(page, /^Done: /).first()).toBeVisible();
    await page.waitForTimeout(1200);
    expect(threads.filter((id) => id === done.thread_id)).toHaveLength(1);
  });
});

interface TodoAnswer {
  id: string;
  title: string;
  thread_id?: string | null;
}
interface RunwayAnswer {
  runway: {
    now: TodoAnswer[];
    coming_up: { todos: TodoAnswer[] }[];
  };
}

test.describe("To do: ticking off with the email open", () => {
  let mutations: Tracked;
  test.beforeEach(({ page }) => {
    mutations = trackMutations(page);
  });
  test.afterEach(async ({ page }) => {
    await undoAll(page, mutations);
  });

  /** Rows in the order the page shows them, each with its email's thread. */
  async function runway(page: Page) {
    let rows: TodoAnswer[] = [];
    await expect
      .poll(
        async () => {
          const answer = (await bridge<RunwayAnswer>(page, "/api/v1/mail/todos")).runway;
          rows = [...answer.now, ...answer.coming_up.flatMap((week) => week.todos)];
          return rows.filter((row) => row.thread_id).length;
        },
        { timeout: 60_000 },
      )
      .toBeGreaterThan(1);
    return rows.filter((row): row is TodoAnswer & { thread_id: string } => Boolean(row.thread_id));
  }

  async function openEmailOf(page: Page, todo: TodoAnswer) {
    const row = page.locator(`[data-testid="todo-row"][data-id="${todo.id}"]`);
    await row.click();
    await expect(row).toHaveAttribute("aria-current", "true");
    await page.keyboard.press("o");
    await row.getByRole("button", { name: "Open the email" }).click();
    await expect(page).toHaveURL(new RegExp(`/todo/${todo.thread_id}$`));
    await expect(reader(page)).toBeVisible();
    // Back to the list, with the email still open beside it.
    await page.keyboard.press("h");
  }

  test("the next row's email opens, and u brings the ticked one back", async ({ page }) => {
    const rows = await runway(page);
    const [first, second] = rows;
    await openApp(page, "/todo");
    await openEmailOf(page, first!);
    await page.keyboard.press("e");
    await expect(page).toHaveURL(new RegExp(`/todo/${second!.thread_id}$`));
    await expect(page.locator(`[data-testid="todo-row"][data-id="${second!.id}"]`)).toHaveAttribute(
      "aria-current",
      "true",
    );

    await page.keyboard.press("u");
    await expect(page).toHaveURL(new RegExp(`/todo/${first!.thread_id}$`));
    await expect(page.locator(`[data-testid="todo-row"][data-id="${first!.id}"]`)).toBeVisible();
  });

  test("the last row's email goes to the one before", async ({ page }) => {
    const rows = await runway(page);
    const last = rows.at(-1)!;
    const before = rows.at(-2)!;
    await openApp(page, "/todo");
    await openEmailOf(page, last);
    await page.keyboard.press("e");
    await expect(page).toHaveURL(new RegExp(`/todo/${before.thread_id}$`));
  });
});

test.describe("Inbox: the reader after archive", () => {
  let mutations: Tracked;
  test.beforeEach(({ page }) => {
    mutations = trackMutations(page);
  });
  test.afterEach(async ({ page }) => {
    await undoAll(page, mutations);
  });

  async function inboxThreads(page: Page): Promise<string[]> {
    const ids = await mailList(page)
      .getByRole("option")
      .evaluateAll((nodes) => nodes.map((node) => node.id));
    return ids;
  }

  test("archive from the list with the reader open opens the next, and u reopens it", async ({
    page,
  }) => {
    await openList(page, "/m/inbox");
    const ids = await inboxThreads(page);
    await page.keyboard.press("j");
    await expectCursorOn(page, ids[1]!);
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
    const openedUrl = page.url();
    const openedTitle = await reader(page).getAttribute("aria-label");
    await page.keyboard.press("h");
    await page.keyboard.press("e");
    await expect(page).not.toHaveURL(openedUrl);
    await expect(reader(page)).not.toHaveAttribute("aria-label", openedTitle!);
    await expect(reader(page)).toBeVisible();

    await page.keyboard.press("u");
    await expect(page).toHaveURL(openedUrl);
    await expect(reader(page)).toHaveAttribute("aria-label", openedTitle!);
  });

  test("archive in the reader on the last row goes to the one before, and u reopens it", async ({
    page,
  }) => {
    await openList(page, "/m/inbox");
    await page.keyboard.press("G");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
    const lastUrl = page.url();
    const lastTitle = await reader(page).getAttribute("aria-label");
    await page.keyboard.press("e");
    await expect(page).not.toHaveURL(lastUrl);
    await expect(reader(page)).toBeVisible();
    await expect(reader(page)).not.toHaveAttribute("aria-label", lastTitle!);

    await page.keyboard.press("u");
    await expect(page).toHaveURL(lastUrl);
  });

  test("a conversation archived elsewhere, seen at the next sync, hands the reader to the next one", async ({
    page,
  }) => {
    await openList(page, "/m/inbox");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/m\/inbox\/[^/]+$/);
    const thread = decodeURIComponent(page.url().split("/").at(-1)!);
    const openedUrl = page.url();
    const { messages } = await bridge<{ messages: { id: string }[] }>(
      page,
      `/api/v1/mail/threads/${encodeURIComponent(thread)}`,
    );
    // Another client archives the whole conversation.
    const result = await bridge<MutationAnswer>(page, "/api/v1/mail/mutations/archive", {
      message_ids: messages.map((message) => message.id),
    });
    const archived = idOf(result);
    if (archived) mutations.pending.push(Promise.resolve(archived));
    // This page hears about it the way it hears about any outside change:
    // a sync finishing refetches the lists.
    await bridge(page, "/api/v1/mail/sync", {});
    await expect(page).not.toHaveURL(openedUrl, { timeout: 15_000 });
    await expect(reader(page)).toBeVisible();
  });
});

test.describe("Now and Updates", () => {
  let mutations: Tracked;
  test.beforeEach(({ page }) => {
    mutations = trackMutations(page);
  });
  test.afterEach(async ({ page }) => {
    await undoAll(page, mutations);
  });

  test("done on a Now row puts the cursor on the next row, and u puts it back", async ({
    page,
  }) => {
    await openApp(page, "/");
    const rows = page.getByTestId("now-row");
    await expect(rows.nth(2)).toBeVisible({ timeout: 60_000 });
    await expect(rows.nth(1)).toHaveAttribute("data-kind", "person");
    const label = (index: number) =>
      rows.nth(index).getByTestId("now-open").getAttribute("aria-label");
    const row = (name: string) =>
      rows.filter({ has: page.locator(`[data-testid="now-open"][aria-label="${name}"]`) });
    const second = (await label(1))!;
    const third = (await label(2))!;
    await page.keyboard.press("j");
    await expect(row(second)).toHaveAttribute("data-focused", "true");
    await page.keyboard.press("e");
    await expect(row(second)).toHaveCount(0);
    await expect(row(third)).toHaveAttribute("data-focused", "true");
    await page.keyboard.press("u");
    await expect(row(second)).toHaveAttribute("data-focused", "true", { timeout: 15_000 });
  });

  test("done in Updates with a message open previews, then opens the next source's email", async ({
    page,
  }) => {
    await openApp(page, "/updates");
    const lines = page.getByTestId("update-line");
    await expect(lines.nth(1)).toBeVisible({ timeout: 60_000 });
    // Open the first line's email; a source's let go previews first (Updates
    // never lets go of a source unseen), then the next source's email opens.
    const firstSource = (await lines.first().getByTestId("update-source").textContent())!.trim();
    await lines.first().hover();
    await page.keyboard.press("o");
    await expect(page).toHaveURL(/\/updates\/[^/]+$/);
    const openedUrl = page.url();
    await page.keyboard.press("h");
    await page.keyboard.press("e");
    const preview = page.getByTestId("updates-let-go-dialog");
    await expect(preview).toContainText("from 1 source");
    await preview.getByTestId("updates-let-go-confirm").click();
    await expect(page).not.toHaveURL(openedUrl, { timeout: 15_000 });
    await expect(page).toHaveURL(/\/updates\/[^/]+$/);
    // The let go's own toast, then u puts the source back.
    await expect(page.locator("[data-sonner-toast]").first()).toContainText(/^Let go of \d+/);
    await page.keyboard.press("u");
    await expect(page.getByText(/^Undone$/)).toBeVisible({ timeout: 15_000 });
    await expect(lines.filter({ hasText: firstSource })).toHaveCount(1, { timeout: 15_000 });
  });
});
