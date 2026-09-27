import { expect, test, type Page } from "@playwright/test";

import { openList, reader, threadMessages } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

// The demo mail has no asks or trackers, so these specs give the newest
// message of the opened conversation one of each. The daemon's response
// shape is kept; only the body changes.
const ASK = "Can you confirm who owns the rollout check before Monday?";
const BODY_TEXT = `Rollout risk: watch sync latency and auth failures for the first hour.\n${ASK}\n\nThanks,\nAlex`;
const BODY_HTML =
  `<p>Rollout risk: watch sync latency and auth failures for the first hour.</p>` +
  `<p>Can you confirm who owns the <b>rollout check</b> before Monday?</p>` +
  `<img src="https://acme.list-manage.com/track/open.php?u=1" width="1" height="1">` +
  `<img src="https://mcusercontent.com/e2e/hero.png" width="600">`;

const isThreadRead = (url: URL) => /^\/api\/v1\/mail\/threads\/[^/]+$/.test(url.pathname);

/** Rewrite the newest body; returns a getter for that message's id. */
async function stubNewestBody(page: Page): Promise<() => string> {
  let newestId = "";
  await page.route(isThreadRead, async (route) => {
    const response = await route.fetch();
    const json = await response.json();
    const newest = json.bodies?.at(-1);
    if (newest) {
      newest.text_plain = BODY_TEXT;
      newest.text_html = BODY_HTML;
      newestId = newest.message_id;
    }
    await route.fulfill({ response, json });
  });
  return () => newestId;
}

/**
 * The e2e daemon has no model, so the model is stubbed at the bridge
 * boundary: the LLM status says one is configured, and the gist route
 * answers late, with the ask quoting the newest message. The daemon's own
 * gist path (prompt, quote check, cache, privacy) is covered by its tests.
 */
async function stubModel(page: Page, newestId: () => string, delayMs: number): Promise<void> {
  await page.route("**/api/v1/platform/llm/status", (route) =>
    route.fulfill({
      json: { kind: "LlmStatus", status: { enabled: true, provider: "stub", model: "stub-7b" } },
    }),
  );
  await page.route("**/api/v1/mail/threads/*/context/gist**", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, delayMs));
    const threadId = new URL(route.request().url()).pathname.split("/").at(-3);
    await route.fulfill({
      json: {
        kind: "ThreadGist",
        gist: {
          thread_id: threadId,
          status: "ready",
          gist: "Canary stays at 5% until the dashboard is quiet; Alex needs an owner for the rollout check.",
          ask: {
            summary: "confirm who owns the rollout check",
            quote: { message_id: newestId(), text: ASK },
          },
          provenance: { model: "stub-7b", locality: "local", sources: ["this_thread"] },
          from_cache: false,
        },
      },
    });
  });
}

async function openFirstConversation(page: Page): Promise<void> {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  await expect(threadMessages(page).first()).toBeVisible();
}

function composer(page: Page) {
  return page.locator("#inline-composer-slot [data-compose-surface]");
}

test("facts are there as the thread opens, and with no model there is no AI slot", async ({
  page,
}) => {
  await openFirstConversation(page);
  // Checked at once, not waited for: the reader paints with its facts.
  expect(await page.getByTestId("thread-context-facts").isVisible()).toBe(true);
  await expect(page.getByTestId("thread-context-facts")).toHaveText(/^You and \S+: \d+ emails?/);
  await expect(page.getByTestId("thread-gist")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Draft in your voice/ })).toHaveCount(0);
});

test("the ask lands without moving the messages and is marked in the message", async ({ page }) => {
  const trackerHits: string[] = [];
  await page.context().route("https://acme.list-manage.com/**", async (route) => {
    trackerHits.push(route.request().url());
    await route.abort();
  });
  const newestId = await stubNewestBody(page);
  await stubModel(page, newestId, 1_200);
  await openFirstConversation(page);

  const slot = page.getByTestId("thread-gist");
  await expect(slot).toHaveAttribute("data-state", "loading");
  const first = threadMessages(page).first();
  const before = await first.boundingBox();
  await expect(slot).toHaveAttribute("data-state", "ready");
  // Let the fade finish, then measure again: nothing below the slot moved.
  await page.waitForTimeout(400);
  const after = await first.boundingBox();
  expect(after?.y).toBe(before?.y);

  await expect(page.getByTestId("thread-ask")).toContainText("Asks you confirm who owns");
  await expect(page.getByTestId("thread-gist-source")).toHaveText(
    "Local model stub-7b · from this thread",
  );

  // Formatted view: the mark is inside the sanitized frame, split around <b>.
  const frame = reader(page).frameLocator('iframe[title="HTML message body"]').last();
  await expect(frame.locator("mark[data-ask-quote]").first()).toBeVisible();
  const marked = await frame
    .locator("mark[data-ask-quote]")
    .evaluateAll((marks) => marks.map((mark) => mark.textContent).join(""));
  expect(marked).toBe(ASK);

  // Reader view marks it in the page; "Show in message" scrolls to it.
  await page.keyboard.press("R");
  const mark = reader(page).locator("mark[data-ask-quote]");
  await expect(mark).toHaveText(ASK);
  await page.getByTestId("thread-scroll").evaluate((node) => node.scrollTo({ top: 0 }));
  await page.getByRole("button", { name: "Show in message" }).click();
  await expect(mark).toBeInViewport();
  expect(trackerHits).toEqual([]);

  // With a model, the reply field offers the composer's draft in your voice.
  await page.getByRole("button", { name: "Draft in your voice" }).click();
  await expect(composer(page).getByLabel("What should this say?")).toBeVisible();
  await composer(page).getByRole("button", { name: "Close composer (saves draft)" }).click();
});

test("r opens the reply at the end of the thread, in view; so does the field", async ({ page }) => {
  await openFirstConversation(page);
  const field = page.getByTestId("reply-field");
  await expect(field).toBeVisible();
  // No reply buttons in the toolbar above the thread any more.
  await expect(
    reader(page)
      .locator("header")
      .first()
      .getByRole("button", { name: /^Reply/ }),
  ).toHaveCount(0);

  await page.getByTestId("thread-scroll").evaluate((node) => node.scrollTo({ top: 0 }));
  await page.keyboard.press("r");
  await expect(composer(page)).toBeVisible();
  await expect(composer(page)).toBeInViewport();
  await expect(field).toHaveCount(0);
  // The composer sits after the last message.
  const lastMessage = await threadMessages(page).last().boundingBox();
  const composerBox = await composer(page).boundingBox();
  expect(composerBox?.y ?? 0).toBeGreaterThan(lastMessage?.y ?? Infinity);

  await composer(page).getByRole("button", { name: "Close composer (saves draft)" }).click();
  await expect(field).toBeVisible();
  await field.getByRole("button", { name: /^Reply to / }).click();
  await expect(composer(page)).toBeVisible();
  await expect(composer(page)).toBeInViewport();
  await composer(page).getByRole("button", { name: "Close composer (saves draft)" }).click();
});

test("the privacy line names what was blocked and from whom", async ({ page }) => {
  const imageHits: string[] = [];
  await page.context().route("https://mcusercontent.com/**", async (route) => {
    imageHits.push(route.request().url());
    await route.fulfill({ status: 200, contentType: "image/png", body: Buffer.alloc(0) });
  });
  const trackerHits: string[] = [];
  await page.context().route("https://acme.list-manage.com/**", async (route) => {
    trackerHits.push(route.request().url());
    await route.abort();
  });
  await stubNewestBody(page);
  await openFirstConversation(page);

  const line = reader(page).getByTestId("privacy-line");
  await expect(line).toContainText("Blocked 1 tracker and 1 remote image from Mailchimp.");
  expect(imageHits).toEqual([]);
  await line.getByRole("button", { name: /^Show images/ }).click();
  await expect(line).toContainText("Blocked 1 tracker from Mailchimp.");
  await expect.poll(() => imageHits.length).toBeGreaterThan(0);
  expect(trackerHits).toEqual([]);
});
