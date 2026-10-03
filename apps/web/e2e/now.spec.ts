import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface NowAnswer {
  now: {
    item_count: number;
    people: { rows: { row: { thread_id: string; counterparty_name?: string } }[] };
    due_soon: { todos: { todo: { id: string; thread_id?: string; title: string } }[] };
  };
}

/** Wait for the first run, so Now and To do have their rows. */
async function waitForNow(page: Page) {
  await expect
    .poll(async () => (await bridge<NowAnswer>(page, "/api/v1/mail/now")).now.item_count, {
      timeout: 60_000,
    })
    .toBeGreaterThan(0);
}

test("Now is the front page: / and g h open it, with its job in one line", async ({ page }) => {
  await waitForNow(page);
  await openApp(page, "/");
  await expect(page).toHaveURL(/\/now$/);
  await expect(page.getByRole("heading", { name: "Now", level: 1 })).toBeVisible();
  await expect(page.getByTestId("mode-header")).toHaveText(
    "The few things that need you now, from every mode.",
  );
  await page.keyboard.press("g");
  await page.keyboard.press("i");
  await expect(page).toHaveURL(/\/m\/inbox$/);
  await page.keyboard.press("g");
  await page.keyboard.press("h");
  await expect(page).toHaveURL(/\/now$/);
});

test("each section shows at most three items and Now at most ten", async ({ page }) => {
  await waitForNow(page);
  await openApp(page, "/now");
  await expect(page.getByTestId("now-row").first()).toBeVisible();
  for (const section of ["people", "due", "reading"]) {
    const rows = page.getByTestId(`now-section-${section}`).getByTestId("now-row");
    expect(await rows.count(), section).toBeLessThanOrEqual(3);
  }
  expect(await page.getByTestId("now-section-updates").count()).toBeLessThanOrEqual(1);
  expect(await page.getByTestId("now-row").count()).toBeLessThanOrEqual(10);
  // A section with more says where the rest are, in the daemon's words.
  for (const more of await page.getByTestId("now-more").all()) {
    await expect(more).toHaveText(/^and \d+ more in (Messages|To do)$/);
  }
});

interface DeskAnswer {
  owed: { rows: { thread_id: string; counterparty_name?: string }[] };
  people_new: { rows: { thread_id: string; counterparty_name?: string }[] };
}

interface MembershipAnswer {
  threads: { modes: { mode: string; todo_ids?: string[] }[]; new_sender?: { question: string } }[];
}

/** The landlord's thread, from the desk Messages is built on. */
async function samThread(page: Page): Promise<string> {
  const desk = await bridge<DeskAnswer>(page, "/api/v1/mail/desk?lane_limit=5000");
  const row = desk.owed.rows.find((candidate) => candidate.counterparty_name === "Sam Okafor");
  expect(row, "Sam's thread is your turn").toBeTruthy();
  return row!.thread_id;
}

async function modesOf(page: Page, threadId: string) {
  return (
    await bridge<MembershipAnswer>(page, `/api/v1/mail/modes/membership?thread_id=${threadId}`)
  ).threads[0]!;
}

test("a first-time person is asked one question on their row", async ({ page }) => {
  await waitForNow(page);
  await openApp(page, "/messages?lane=people_new");
  const noor = page.getByRole("option").filter({ hasText: "Noor Haddad" });
  await expect(noor).toBeVisible();
  await expect(noor.getByTestId("new-sender-question")).toContainText(
    "New sender. Keep in Messages?",
  );
});

test("e in Messages leaves the to-do open, says where it still is, and u undoes it", async ({
  page,
}) => {
  await waitForNow(page);
  const thread = await samThread(page);
  const before = await modesOf(page, thread);
  const todoIds = before.modes.find((entry) => entry.mode === "todo")?.todo_ids ?? [];
  expect(todoIds.length, "Sam's email is also a to-do").toBeGreaterThan(0);

  await openApp(page, "/messages?lane=owed");
  const sam = page.getByRole("option").filter({ hasText: "Sam Okafor" });
  await expect(sam).toBeVisible();
  // Messages names the other mode holding the thread.
  await expect(sam.getByTestId("also-in")).toContainText("Also in To do:");
  await sam.getByTestId("desk-done").click();
  const toast = page.locator("[data-sonner-toast]").filter({ hasText: "Done in Messages." });
  await expect(toast).toContainText("Still in To do");
  await expect(page.getByRole("option").filter({ hasText: "Sam Okafor" })).toHaveCount(0);
  // The to-do is still open, and the thread is out of Messages only.
  const after = await modesOf(page, thread);
  expect(after.modes.map((entry) => entry.mode)).toContain("todo");
  expect(after.modes.map((entry) => entry.mode)).not.toContain("messages");

  await page.keyboard.press("u");
  await expect(page.locator("[data-sonner-toast]").filter({ hasText: /^Undone/ })).toBeVisible();
  await expect(page.getByRole("option").filter({ hasText: "Sam Okafor" })).toBeVisible();
  expect((await modesOf(page, thread)).modes.map((entry) => entry.mode)).toContain("messages");
});

test("the reader names the other modes holding a conversation", async ({ page }) => {
  await waitForNow(page);
  const thread = await samThread(page);
  await openApp(page, `/messages/${thread}`);
  const line = page.getByRole("article").getByTestId("also-in");
  await expect(line).toContainText("Also in To do:");
  await expect(line).not.toContainText("Also in Messages");
  await expect(line.getByRole("link")).toHaveAttribute("href", "/todo");
});

test("the rail lists Now, the modes, then Inbox, and their keys open them", async ({ page }) => {
  await waitForNow(page);
  await openApp(page, "/now");
  const rail = page.getByRole("navigation", { name: "Places" });
  await expect(rail.getByRole("link")).toHaveText([
    /^Now/,
    /^Messages/,
    /^To do/,
    /^Updates/,
    /^Reading/,
    /^Archive/,
    /^Inbox/,
  ]);
  // Early modes say so.
  await expect(
    rail.getByRole("link", { name: /^Updates/ }).getByTestId("rail-early"),
  ).toBeVisible();
  for (const [key, url] of [
    ["m", /\/messages$/],
    ["x", /\/todo$/],
    ["u", /\/updates$/],
    ["r", /\/reading$/],
    ["e", /\/archive$/],
    ["i", /\/m\/inbox$/],
    ["h", /\/now$/],
  ] as const) {
    await page.keyboard.press("g");
    await page.keyboard.press(key);
    await expect(page, `g ${key}`).toHaveURL(url);
  }
  // The pages the modes replaced are under More.
  const more = page.getByRole("navigation", { name: "More" });
  await page.getByRole("button", { name: "More" }).click();
  for (const name of ["Screener", "Reply queue", "Waiting on", "Snoozed", "Subscriptions"]) {
    await expect(more.getByRole("link", { name: new RegExp(`^${name}`) })).toBeVisible();
  }
});

test("Paper trail's old address opens Updates", async ({ page }) => {
  await openApp(page, "/paper-trail");
  await expect(page).toHaveURL(/\/updates$/);
  await expect(page.getByTestId("early-mode-note")).toContainText("early version");
});

test("Now's card shows once: Esc retires it here, after a reload and for the TUI", async ({
  page,
}) => {
  await waitForNow(page);
  await bridge(page, "/api/v1/mail/modes/now/card", { seen: false });
  await openApp(page, "/now");
  const card = page.getByTestId("mode-card");
  await expect(card).toBeVisible();
  await expect(card).toContainText("Now shows at most ten things");
  await expect(card).toContainText("e done here");
  await page.keyboard.press("Escape");
  await expect(card).toHaveCount(0);
  await page.reload();
  await expect(page.getByTestId("now-row").first()).toBeVisible();
  await expect(page.getByTestId("mode-card")).toHaveCount(0);
  const guide = await bridge<{ guides: { card_seen: boolean }[] }>(
    page,
    "/api/v1/mail/modes/guide?mode=now",
  );
  expect(guide.guides[0]!.card_seen).toBe(true);
});

test("? on Now leads with what Now is for", async ({ page }) => {
  await waitForNow(page);
  await openApp(page, "/now");
  await expect(page.getByTestId("now-row").first()).toBeVisible();
  await page.keyboard.press("?");
  const help = page.getByRole("dialog", { name: "Keyboard" });
  await expect(help.getByTestId("mode-help")).toContainText(
    "Now: The few things that need you now",
  );
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  async function overflow(page: Page): Promise<number> {
    return page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
  }

  test("five tabs, no sidebar and no sideways scroll", async ({ page }) => {
    await waitForNow(page);
    await openApp(page, "/now");
    const tabs = page.getByTestId("mobile-tabs");
    await expect(tabs.getByRole("link")).toHaveText([
      /Now/,
      /Messages/,
      /To do/,
      /Reading/,
      /Find/,
    ]);
    await expect(page.getByRole("complementary", { name: "Mailboxes" })).toBeHidden();
    await expect(page.getByTestId("now-row").first()).toBeVisible();
    await expect(page.locator(".app-shell-statusbar")).toBeHidden();
    // The page gets the whole width, not a strip beside hidden columns.
    const main = (await page.locator("#main").boundingBox())!;
    expect(main.width).toBeGreaterThanOrEqual(389);
    for (const [tab, url] of [
      ["Messages", /\/messages$/],
      ["To do", /\/todo$/],
      ["Reading", /\/reading$/],
      ["Find", /\/find$/],
      ["Now", /\/now$/],
    ] as const) {
      await tabs.getByRole("link", { name: tab }).click();
      await expect(page).toHaveURL(url);
      // Live views poll, so the network never idles: wait for a settled 0.
      await expect.poll(() => overflow(page), { message: tab }).toBe(0);
    }
  });

  test("Find holds Archive, search and the Inbox; Updates opens from Now's card", async ({
    page,
  }) => {
    await waitForNow(page);
    await openApp(page, "/find");
    await expect(page.getByRole("searchbox").or(page.getByLabel("Search all mail"))).toBeVisible();
    await expect(page.getByRole("link", { name: /^Archive/ })).toBeVisible();
    await expect(page.getByRole("link", { name: /^Inbox/ })).toBeVisible();
    await page.getByRole("link", { name: "Now" }).click();
    const card = page.getByTestId("now-section-updates");
    await expect(card).toBeVisible();
    await card.getByRole("link", { name: /Open/ }).click();
    await expect(page).toHaveURL(/\/updates$/);
  });
});
