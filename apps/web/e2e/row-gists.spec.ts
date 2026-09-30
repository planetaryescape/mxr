import { expect, test, type Page, type WebSocketRoute } from "@playwright/test";

import { mailRows, openList } from "./helpers/mail";
import { openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

/*
 * The e2e daemon has no model, so the model is stubbed at the bridge
 * boundary, like reader-context.spec: the LLM status says one is
 * configured, the batch gist route queues every row it is asked about, and
 * the gists land later as daemon events on the real event stream. The
 * daemon's own queue (order, dedup, people only, privacy) has Rust tests.
 */
interface GistStub {
  /** Each POST /api/v1/mail/gists body's thread ids, in call order. */
  asked: string[][];
  /** Send `ThreadGistReady` for a conversation down the event stream. */
  land: (threadId: string, ask: string | null) => void;
}

async function stubGists(page: Page): Promise<GistStub> {
  const asked: string[][] = [];
  let socket: WebSocketRoute | undefined;
  await page.route("**/api/v1/platform/llm/status", (route) =>
    route.fulfill({
      json: {
        kind: "LlmStatus",
        status: { enabled: true, provider: "stub", model: "stub-7b" },
      },
    }),
  );
  await page.route("**/api/v1/mail/gists", async (route) => {
    const { thread_ids } = route.request().postDataJSON() as {
      thread_ids: string[];
    };
    asked.push(thread_ids);
    await route.fulfill({
      json: {
        kind: "ThreadGists",
        batch: {
          model: "available",
          gists: [],
          queued: thread_ids,
          skipped: [],
        },
      },
    });
  });
  await page.routeWebSocket(/\/api\/v1\/events/, (ws) => {
    ws.connectToServer();
    socket = ws;
  });
  return {
    asked,
    land: (threadId, ask) => {
      if (!socket) throw new Error("the app never opened the event stream");
      socket.send(
        JSON.stringify({
          event: "ThreadGistReady",
          gist: {
            thread_id: threadId,
            status: "ready",
            gist: "Canary stays at 5% until the dashboard is quiet.",
            ask: ask ? { summary: ask } : null,
            provenance: {
              model: "stub-7b",
              locality: "local",
              sources: ["this_thread"],
            },
            from_cache: false,
          },
        }),
      );
    },
  };
}

async function rowBoxes(page: Page) {
  return mailRows(page).evaluateAll((rows) =>
    rows.map((row) => {
      const box = row.getBoundingClientRect();
      return { top: Math.round(box.top), height: Math.round(box.height) };
    }),
  );
}

test("desk rows say what each conversation asks, as the gists land, without moving", async ({
  page,
}) => {
  const stub = await stubGists(page);
  await openApp(page);
  await expect(mailRows(page).first()).toBeVisible();
  // One request, for the rows on screen, top row first.
  await expect.poll(() => stub.asked.length).toBe(1);
  const [first, second] = stub.asked[0]!;
  expect(first && second).toBeTruthy();

  // With a model, every row keeps its gist line from the start.
  const lines = page.getByTestId("desk-gist");
  await expect(lines.first()).toHaveAttribute("data-state", "waiting");
  expect(await lines.count()).toBe(await mailRows(page).count());
  const before = await rowBoxes(page);

  stub.land(first!, "confirm who owns the rollout check");
  stub.land(second!, null);
  const top = mailRows(page).first();
  await expect(top.getByTestId("desk-ask")).toHaveText("Asks: confirm who owns the rollout check");
  await expect(top.getByTestId("desk-gist")).toHaveText(
    "Canary stays at 5% until the dashboard is quiet.",
  );
  await expect(top.getByTestId("desk-gist")).toHaveAttribute("title", /Local model stub-7b/);
  await expect(top).toHaveAttribute("aria-label", /Asks you to confirm who owns/);
  // The second conversation has no ask: its reason stays, its gist shows.
  await expect(page.locator('[data-testid="desk-gist"][data-state="ready"]')).toHaveCount(2);
  await expect(page.getByTestId("desk-ask")).toHaveCount(1);

  expect(await rowBoxes(page)).toEqual(before);
  // Nothing new is asked for rows already asked about.
  await page.waitForTimeout(400);
  expect(stub.asked).toHaveLength(1);
});

test("inbox rows show the gist in place of the snippet, in the same box", async ({ page }) => {
  const stub = await stubGists(page);
  await openList(page, "/m/inbox");
  await expect.poll(() => stub.asked.length).toBe(1);
  const top = mailRows(page).first();
  const before = await rowBoxes(page);

  stub.land(stub.asked[0]![0]!, "send the signed contract");
  const line = top.getByTestId("row-gist");
  await expect(line).toHaveText(
    "Asks: send the signed contract · Canary stays at 5% until the dashboard is quiet.",
  );
  await expect(line).toHaveAttribute("title", /Local model stub-7b · from this thread/);
  expect(await rowBoxes(page)).toEqual(before);

  // Scrolling asks for the rows that come into view, never the same twice.
  await page.getByTestId("mailbox-list").evaluate((list) => list.scrollBy(0, 2_000));
  await expect.poll(() => stub.asked.length).toBe(2);
  const [firstAsk, secondAsk] = stub.asked;
  expect(secondAsk!.some((id) => firstAsk!.includes(id))).toBe(false);
});

test("with no model, rows look as they always did", async ({ page }) => {
  const posts: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("/api/v1/mail/gists")) posts.push(request.url());
  });
  await openApp(page);
  await expect(mailRows(page).first()).toBeVisible();
  await expect(page.getByTestId("desk-gist")).toHaveCount(0);
  await expect(page.getByTestId("desk-ask")).toHaveCount(0);
  await expect(mailRows(page).first()).toHaveAttribute(
    "aria-label",
    /replied to your message|wrote to you|messages since you last wrote|copied you/,
  );

  await openList(page, "/m/inbox");
  await expect(mailRows(page).first()).toBeVisible();
  await page.waitForTimeout(400);
  await expect(page.getByTestId("row-gist")).toHaveCount(0);
  // The daemon said once that there is no model; the lists stopped asking.
  expect(posts.length).toBeLessThanOrEqual(1);
});
