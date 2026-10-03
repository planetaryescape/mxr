import { expect, test, type Locator, type Page } from "@playwright/test";

import { openList, reader, threadMessages } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

const isThreadRead = (url: URL) => /^\/api\/v1\/mail\/threads\/[^/]+$/.test(url.pathname);

interface ThreadRead {
  thread: { id: string };
  messages: { id: string; date: string }[];
}

/**
 * Open the list's first conversation with at least `minMessages` messages
 * and return what the reader loaded. The demo's newest mail changes as
 * fixtures are added, so this walks down the list rather than trusting
 * the top row.
 */
async function openFirst(page: Page, path: string, minMessages = 1): Promise<ThreadRead> {
  await openList(page, path);
  for (let row = 0; row < 15; row += 1) {
    const read = page.waitForResponse((response) => isThreadRead(new URL(response.url())));
    await page.keyboard.press("Enter");
    const thread = (await (await read).json()) as ThreadRead;
    await expect(threadMessages(page).first()).toBeVisible();
    if (thread.messages.length >= minMessages) return thread;
    await page.keyboard.press("Escape");
    await page.keyboard.press("j");
  }
  throw new Error(`no conversation in ${path} has ${minMessages} messages`);
}

async function openSource(page: Page, link: Locator): Promise<Page> {
  const [tab] = await Promise.all([page.context().waitForEvent("page"), link.click()]);
  await tab.waitForLoadState();
  return tab;
}

/** The reader opened on this message: expanded, focused and in view. */
async function expectLandedOn(tab: Page, messageId: string): Promise<void> {
  const card = reader(tab).locator(`[data-message-id="${messageId}"]`);
  await expect(card).toBeVisible();
  await expect(card).not.toHaveAttribute("data-collapsed", "true");
  await expect(card).toHaveAttribute("data-focused", "true");
  await expect(card).toBeInViewport();
}

function composer(page: Page) {
  return page.locator("[data-compose-surface]");
}

/**
 * The e2e daemon has no model, so the model is stubbed at the bridge: the
 * status says one is configured, and the draft route answers with a body
 * and the provenance the daemon would send. The daemon's own provenance
 * (pinned provider, privacy gate, sources equal to the prompt's examples)
 * is covered by its tests; this checks the user can see and follow it.
 */
test("a drafted reply says which model wrote it, and each source opens the cited message", async ({
  page,
}) => {
  await page.route("**/api/v1/platform/llm/status", (route) =>
    route.fulfill({
      json: {
        kind: "LlmStatus",
        status: { enabled: true, provider: "stub", model: "stub-7b" },
      },
    }),
  );

  // One of your own emails, from Sent, stands in for a voice example.
  const sent = await openFirst(page, "/m/sent");
  const yourEmail = sent.messages.at(-1);
  if (!yourEmail) throw new Error("the demo's Sent list opened an empty conversation");
  // The conversation being answered; its oldest message is cited too.
  const inbox = await openFirst(page, "/m/inbox", 2);
  const oldest = inbox.messages[0];
  if (!oldest) throw new Error("the demo's inbox opened an empty conversation");
  expect(inbox.messages.length).toBeGreaterThan(1);
  // Folded until something asks for it.
  await expect(reader(page).locator(`[data-message-id="${oldest.id}"]`)).toHaveAttribute(
    "data-collapsed",
    "true",
  );

  let draftRequest: Record<string, unknown> | null = null;
  await page.route("**/api/v1/mail/drafts/compose", async (route) => {
    draftRequest = route.request().postDataJSON() as Record<string, unknown>;
    await route.fulfill({
      json: {
        kind: "DraftSuggestion",
        body: "Friday works. I'll send the notes before then.",
        model: "stub-7b",
        rewrite_iterations: 0,
        provenance: {
          model: "stub-7b",
          locality: "local",
          history_used: true,
          voice_examples: [
            {
              message_id: yourEmail.id,
              thread_id: sent.thread.id,
              date: yourEmail.date,
              from_me: true,
              person: "maya@example.com",
              person_name: "Maya Chen",
            },
          ],
          conversation: [
            {
              message_id: oldest.id,
              thread_id: inbox.thread.id,
              date: oldest.date,
              from_me: false,
              person: "maya@example.com",
              person_name: "Maya Chen",
            },
          ],
        },
      },
    });
  });

  await page.getByRole("button", { name: "Draft in your voice" }).click();
  await composer(page).getByRole("button", { name: "Generate" }).click();
  await expect.poll(() => draftRequest).not.toBeNull();
  expect(draftRequest).toHaveProperty("source_message_id");

  const provenance = composer(page).getByTestId("draft-provenance");
  await expect(provenance.getByTestId("draft-provenance-line")).toHaveText(
    "Local model stub-7b · used 1 of your emails to Maya · history used",
  );
  await provenance.getByRole("button", { name: "Sources (2)" }).click();

  // Each source opens the cited message in the reader, in a new tab, so the
  // draft being judged stays where it is.
  const fromThisConversation = await openSource(
    page,
    provenance
      .getByRole("region", { name: "Messages it read from this conversation" })
      .getByRole("link", { name: /^Maya/ }),
  );
  await expect(fromThisConversation).toHaveURL(
    new RegExp(`/m/archive/${inbox.thread.id}\\?message=${oldest.id}$`),
  );
  await expectLandedOn(fromThisConversation, oldest.id);

  const yours = await openSource(
    page,
    provenance
      .getByRole("region", { name: "Your emails it matched the voice of" })
      .getByRole("link", { name: /^Your email to Maya/ }),
  );
  await expect(yours).toHaveURL(new RegExp(`/m/sent/${sent.thread.id}\\?message=${yourEmail.id}$`));
  await expectLandedOn(yours, yourEmail.id);

  // The draft never moved.
  await expect(provenance.getByTestId("draft-provenance-line")).toBeInViewport();
  await expect(page).toHaveURL(new RegExp(`/m/inbox/${inbox.thread.id}`));
});
