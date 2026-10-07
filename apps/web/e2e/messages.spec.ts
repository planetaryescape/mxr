import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

const SAMIR = "person:samir@launchpad.example";
const JON = "person:jon@papertrail.example";
const IRIS = "person:iris@meridian.example";

interface Row {
  id: string;
  title: string;
  band: string;
  kind: string;
  topics: { thread_id: string; subject: string }[];
}

interface MessagesAnswer {
  messages: { your_turn: Row[]; pinned: Row[]; recent: Row[]; quiet: Row[] };
}

async function messages(page: Page) {
  return (await bridge<MessagesAnswer>(page, "/api/v1/mail/people")).messages;
}

function all(data: MessagesAnswer["messages"]): Row[] {
  return [...data.your_turn, ...data.recent, ...data.quiet];
}

/** Wait for the demo's people to sync. */
async function waitForPeople(page: Page) {
  await expect
    .poll(async () => all(await messages(page)).some((row) => row.id === SAMIR), {
      timeout: 60_000,
    })
    .toBe(true);
}

async function topicOf(page: Page, person: string, subject: string): Promise<string> {
  const row = all(await messages(page)).find((candidate) => candidate.id === person);
  const topic = row?.topics.find((candidate) => candidate.subject === subject);
  expect(topic, `${person} has a topic "${subject}"`).toBeTruthy();
  return topic!.thread_id;
}

function row(page: Page, id: string) {
  return page.locator(`[data-testid="messages-row"][data-row-id="${id}"]`);
}

test("each person is one row with topics, a group is its own row, a cc is not here", async ({
  page,
}) => {
  await waitForPeople(page);
  await openApp(page, "/messages");
  await expect(page.getByRole("heading", { name: "Messages", level: 1 })).toBeVisible();
  await expect(page.getByTestId("mode-header").first()).toHaveText(
    "People you talk with, one row each. Reply or mark done.",
  );
  await expect(row(page, SAMIR)).toHaveCount(1);
  await expect(page.getByTestId("band-your_turn").locator(`[data-row-id="${SAMIR}"]`)).toBeVisible();
  await expect(page.getByTestId("messages-row").filter({ hasText: "Samir, Ruth" })).toHaveCount(1);
  await expect(page.getByTestId("messages-row").filter({ hasText: "Offsite dates" })).toHaveCount(0);
  await expect(row(page, IRIS).getByTestId("row-preview")).toHaveText("You: Thanks, on it.");
  // No early-version marker on Messages any more.
  await expect(page.getByTestId("rail-early").filter({ hasText: /messages/i })).toHaveCount(0);

  await row(page, SAMIR).click();
  const pageView = page.getByTestId("person-page");
  await expect(pageView.getByTestId("person-name")).toHaveText("Samir Patel");
  const topics = pageView.getByTestId("topic");
  // The demo's generated mail has "Contract renewal details" and "Launch
  // checklist for Project Aurora" too; these are the seeded topics.
  await expect(topics.filter({ hasText: /Contract renewal(?! details)/ })).toHaveCount(1);
  await expect(topics.filter({ hasText: /Launch checklist(?! for)/ })).toHaveCount(1);
  await expect(topics.filter({ hasText: "with Ruth: Pricing copy" })).toHaveCount(1);
  await expect(pageView.getByTestId("composer")).toContainText("Reply to Samir · ");
});

test("[ and ] step through a person's topics", async ({ page }) => {
  await waitForPeople(page);
  const contract = await topicOf(page, SAMIR, "Contract renewal");
  await openApp(page, `/messages?person=${encodeURIComponent(SAMIR)}&topic=${contract}`);
  const pageView = page.getByTestId("person-page");
  const current = pageView.locator('[data-testid="topic"][aria-current="true"]');
  await expect(current).toContainText("Contract renewal");
  await page.keyboard.press("]");
  await expect(current).not.toContainText("Contract renewal");
  await page.keyboard.press("[");
  await expect(current).toContainText("Contract renewal");
});

test("a long reply is a letter, trimmed, and v shows it as sent", async ({ page }) => {
  await waitForPeople(page);
  const contract = await topicOf(page, SAMIR, "Contract renewal");
  await openApp(page, `/messages?person=${encodeURIComponent(SAMIR)}&topic=${contract}`);
  const letter = page
    .getByTestId("conversation-message")
    .filter({ hasText: "Samir" })
    .filter({ has: page.getByTestId("trimmed-marker") })
    .last();
  await expect(letter).toHaveAttribute("data-layout", "letter");
  await expect(letter.getByTestId("trimmed-marker")).toContainText("trimmed: quote, sig");
  await expect(letter).toContainText(/Read all \d+ paragraphs/);
  // The quote of your own message and Samir's signature are gone.
  await expect(letter).not.toContainText("Could you check it with legal");
  await expect(letter).not.toContainText("+44 20 7946 0102");
  await page.keyboard.press("v");
  await expect(letter.getByTestId("as-sent")).toBeVisible();
  await page.keyboard.press("v");
  await expect(letter.getByTestId("as-sent")).toHaveCount(0);
});

test("Got it shows the exact text and counts down; undo sends nothing", async ({ page }) => {
  await waitForPeople(page);
  const contract = await topicOf(page, SAMIR, "Contract renewal");
  await openApp(page, `/messages?person=${encodeURIComponent(SAMIR)}&topic=${contract}`);
  await expect(page.getByTestId("conversation")).toBeVisible();
  await page.keyboard.press(".");
  const preview = page.getByTestId("got-it-preview");
  await expect(preview).toBeVisible();
  await expect(preview.getByTestId("got-it-text")).toContainText(/got it/i);
  await expect(preview.getByTestId("got-it-countdown")).toHaveText(/Sending in \ds/);
  await page.keyboard.press("u");
  await expect(preview).toHaveCount(0);
  await expect(page.getByText("Got it cancelled. Nothing was sent.")).toBeVisible();
  // Still Samir's turn: nothing went out.
  await page.waitForTimeout(6_000);
  const data = await messages(page);
  expect(data.your_turn.some((candidate) => candidate.id === SAMIR)).toBe(true);
});

test("Got it sends after the countdown and the turn passes", async ({ page }) => {
  await waitForPeople(page);
  const thread = await topicOf(page, JON, "Pricing copy for the docs");
  await openApp(page, `/messages?person=${encodeURIComponent(JON)}&topic=${thread}`);
  await expect(page.getByTestId("conversation")).toBeVisible();
  await page.getByTestId("got-it").click();
  await expect(page.getByTestId("got-it-preview")).toBeVisible();
  await expect(page.getByText(/Got it sent to Jon/)).toBeVisible({ timeout: 15_000 });
  await expect
    .poll(async () => (await messages(page)).your_turn.some((candidate) => candidate.id === JON), {
      timeout: 15_000,
    })
    .toBe(false);
});

test("done here moves a person out of Recent until they write again", async ({ page }) => {
  await waitForPeople(page);
  await openApp(page, `/messages?person=${encodeURIComponent(IRIS)}`);
  await expect(page.getByTestId("person-name")).toHaveText("Iris Chen");
  const topic = await topicOf(page, IRIS, "Incident note");
  await openApp(page, `/messages?person=${encodeURIComponent(IRIS)}&topic=${topic}`);
  await expect(page.getByTestId("conversation")).toBeVisible();
  await page.keyboard.press("e");
  await expect(page.getByText(/Done in Messages|Archived/).first()).toBeVisible();
  await expect
    .poll(async () => (await messages(page)).recent.some((candidate) => candidate.id === IRIS), {
      timeout: 15_000,
    })
    .toBe(false);
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("the list and the person page are two screens", async ({ page }) => {
    await waitForPeople(page);
    await openApp(page, "/messages");
    await expect(row(page, SAMIR)).toBeVisible();
    await expect(page.getByTestId("person-page")).toBeHidden();
    await row(page, SAMIR).click();
    const pageView = page.getByTestId("person-page");
    await expect(pageView).toBeVisible();
    await expect(row(page, SAMIR)).toBeHidden();
    await expect(pageView.getByTestId("got-it")).toBeVisible();
    await pageView.getByRole("button", { name: "Back to the people" }).click();
    await expect(row(page, SAMIR)).toBeVisible();
  });
});
