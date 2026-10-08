import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface Edition {
  bands: {
    band: string;
    items: {
      item_key: string;
      thread_id: string;
      title: string;
      links?: { item_key: string; title: string; domain: string }[];
    }[];
  }[];
  later_count: number;
}

async function edition(page: Page): Promise<Edition> {
  return (await bridge<{ edition: Edition }>(page, "/api/v1/mail/reading")).edition;
}

/** The demo's Reading mail is extracted on the first edition; wait for it. */
async function waitForEdition(page: Page): Promise<Edition> {
  let answer: Edition | undefined;
  await expect
    .poll(
      async () => {
        answer = await edition(page);
        return answer.bands.flatMap((band) => band.items).length;
      },
      { timeout: 60_000 },
    )
    .toBeGreaterThan(0);
  return answer!;
}

function items(page: Page) {
  return page.getByTestId("reading-item");
}

async function openEdition(page: Page) {
  await waitForEdition(page);
  await openApp(page, "/reading");
  await expect(items(page).first()).toBeVisible();
}

/** Serious and critical axe violations; message frames are untrusted mail, not app UI. */
async function blockingViolations(page: Page): Promise<string[]> {
  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa"])
    .exclude("iframe")
    .analyze();
  return results.violations
    .filter((violation) => violation.impact === "serious" || violation.impact === "critical")
    .flatMap((violation) =>
      violation.nodes.map(
        (node) => `${violation.id} (${violation.impact}): ${node.target.join(" ")}`,
      ),
    );
}

test("the edition shows bands with headlines, minutes and reasons, and no unread count", async ({
  page,
}) => {
  await openEdition(page);
  await expect(page.getByRole("heading", { level: 1, name: "Reading", exact: true })).toBeVisible();
  await expect(page.getByTestId("mode-header")).toHaveText(
    "Newsletters you chose, as an edition. Read when you like.",
  );
  await expect(page.getByTestId("band-since_last_visit")).toBeVisible();
  await expect(page.getByTestId("band-fading")).toContainText("Goes within a day");
  const essay = items(page).filter({ hasText: "The quiet death of the three-pane layout" });
  await expect(essay).toBeVisible();
  await expect(essay.getByTestId("reading-meta")).toContainText(/Long Reads Weekly · \d+ min/);
  await expect(essay.getByTestId("why-here").first()).toHaveText(/^Here because: .+/);
  // A digest is its links, each with its site.
  const digest = items(page).filter({ hasText: "six things worth your time" }).first();
  await expect(digest.getByTestId("reading-link")).toHaveCount(4);
  await expect(digest).toContainText("+ 2 more");
  await expect(digest.getByTestId("reading-link").first()).toContainText("links.demo.mxr.local");
  // Nothing here is owed: no unread words, and Later is the only count.
  await expect(page.getByRole("main")).not.toContainText(/unread/i);
  await expect(page.getByTestId("later-count")).toContainText(/Later \d+/);
  // The rail shows Reading built, with no count and no early note.
  await expect(page.getByTestId("early-version")).toHaveCount(0);
  // j moves item to item, then into a digest's links.
  await expect(items(page).first()).toHaveAttribute("aria-current", "true");
  await page.keyboard.press("j");
  await expect(page.locator("[aria-current='true']")).toHaveCount(1);
});

test("Enter reads an item in a 66-character column with time left and a progress line", async ({
  page,
}) => {
  await openEdition(page);
  await items(page)
    .filter({ hasText: "The quiet death of the three-pane layout" })
    .click({
      position: { x: 5, y: 5 },
    });
  await page.keyboard.press("Enter");
  const reader = page.getByTestId("reading-reader");
  await expect(reader).toBeVisible();
  await expect(reader.getByRole("heading", { level: 1 })).toHaveText(
    "The quiet death of the three-pane layout",
  );
  await expect(reader.getByTestId("minutes-left")).toHaveText(/\d+ min left|Read to the end/);
  await expect(reader.getByTestId("reading-progress")).toBeAttached();
  const perLine = await reader.getByTestId("reader-text").evaluate((node) => {
    const paragraph = node.querySelector("p");
    if (!paragraph) return 0;
    const probe = document.createElement("span");
    probe.textContent = "abcdefghijklmnopqrstuvwxyz".repeat(4);
    probe.style.whiteSpace = "nowrap";
    paragraph.append(probe);
    const width = probe.getBoundingClientRect().width / probe.textContent.length;
    probe.remove();
    return paragraph.getBoundingClientRect().width / width;
  });
  expect(perLine).toBeGreaterThanOrEqual(55);
  expect(perLine).toBeLessThanOrEqual(80);
  await expect(reader.getByTestId("end-of-issue")).toContainText("From this source:");
  // Esc goes back to the edition.
  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(/\/reading$/);
});

test("b on a digest link puts it on Later and names the site it saves the article from", async ({
  page,
}) => {
  const before = (await waitForEdition(page)).later_count;
  await openApp(page, "/reading");
  const digest = items(page).filter({ hasText: "six things worth your time" }).first();
  const link = digest.getByTestId("reading-link").filter({ hasText: "WAL checkpoints" });
  await link.click({ position: { x: 2, y: 2 } });
  await expect(link).toHaveAttribute("aria-current", "true");
  await page.keyboard.press("b");
  await expect(
    page.locator("[data-sonner-toast]").filter({ hasText: "Saved to Later" }),
  ).toBeVisible();
  await expect(
    page.getByText(/Fetching from links\.demo\.mxr\.local|Article saved|Couldn't fetch/).first(),
  ).toBeVisible();
  await expect(page.getByTestId("later-count")).toContainText(`Later ${before + 1}`);
  // The Later shelf lists it.
  await page.getByTestId("later-count").click();
  await expect(page).toHaveURL(/view=later/);
  await expect(items(page).filter({ hasText: "WAL checkpoints" })).toBeVisible();
  // Put it back as it was.
  await page.keyboard.press("e");
  await expect(items(page).filter({ hasText: "WAL checkpoints" })).toHaveCount(0);
});

test("L names the site before fetching, and a saved article reads offline", async ({ page }) => {
  await openEdition(page);
  const teaser = items(page).filter({ hasText: "Shipping a sync engine in 2026" }).first();
  await teaser.click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Enter");
  const reader = page.getByTestId("reading-reader");
  await expect(reader.getByRole("radio", { name: "Issue" })).toBeChecked();
  await page.keyboard.press("L");
  await expect(page.getByText("Fetching from platform.demo.mxr.local…").first()).toBeVisible();
  // The demo mailbox serves its own articles, so nothing leaves the machine.
  await expect(reader.getByTestId("article-status")).toContainText("reads offline");
  await expect(reader.getByTestId("reader-text")).toContainText("tombstones for three weeks");
  // Again: the saved copy, with nothing contacted.
  await page.keyboard.press("Escape");
  await teaser.click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Enter");
  await page.keyboard.press("L");
  await expect(reader.getByTestId("article-status")).toContainText("reads offline");
  await expect(reader.getByTestId("reader-text")).toContainText("tombstones for three weeks");
});

test("let go of all previews exactly what it lets go of, and undo brings it back", async ({
  page,
}) => {
  const shown = await waitForEdition(page);
  const threads = new Set(shown.bands.flatMap((band) => band.items.map((item) => item.thread_id)));
  await openApp(page, "/reading");
  await expect(items(page).first()).toBeVisible();
  await page.keyboard.press("A");
  const dialog = page.getByTestId("reading-let-go-all");
  await expect(dialog).toBeVisible();
  const preview = dialog.getByTestId("let-go-all-preview").locator("li");
  await expect(preview).toHaveCount(threads.size);
  const previewed = new Set(
    await preview.evaluateAll((nodes) => nodes.map((node) => node.getAttribute("data-thread"))),
  );
  expect(previewed).toEqual(threads);
  await dialog.getByRole("button", { name: /^Let go of \d+ items?$/ }).click();
  await expect.poll(async () => (await edition(page)).bands.length, { timeout: 20_000 }).toBe(0);
  await expect(page.getByText(/You're current|Nothing here yet/)).toBeVisible();
  await page.keyboard.press("u");
  await expect
    .poll(async () => (await edition(page)).bands.flatMap((band) => band.items).length, {
      timeout: 30_000,
    })
    .toBe(threads.size);
});

test("D previews unsubscribing with the evidence, the method and that it can't be undone", async ({
  page,
}) => {
  await openEdition(page);
  await items(page)
    .filter({ hasText: "Growth Digest" })
    .first()
    .click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("D");
  const dialog = page.getByTestId("reading-unsubscribe");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("heading")).toHaveText("Unsubscribe from Growth Digest?");
  await expect(dialog.getByTestId("unsubscribe-evidence")).not.toBeEmpty();
  await expect(dialog.getByTestId("unsubscribe-method")).toHaveText(
    /One click: the sender is told directly/,
  );
  await expect(dialog.getByTestId("unsubscribe-count")).toHaveText(/Also lets go of \d+ issues?/);
  await expect(dialog.getByTestId("unsubscribe-irreversible")).toHaveText(
    "This can't be undone from mxr; you'd resubscribe on their site.",
  );
  await dialog.getByRole("button", { name: "Keep it" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(items(page).filter({ hasText: "Growth Digest" }).first()).toBeVisible();
});

test("the reader's Unsubscribe button is visible without scrolling and offers both choices", async ({
  page,
}) => {
  await openEdition(page);
  await items(page)
    .filter({ hasText: "Growth Digest" })
    .first()
    .click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Enter");
  const control = page.getByTestId("reader-unsubscribe");
  await expect(control).toBeInViewport();
  await control.click();
  const dialog = page.getByTestId("reading-unsubscribe");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByTestId("unsubscribe-just")).toContainText(
    "Just unsubscribe — keep what you have",
  );
  await expect(dialog.getByTestId("unsubscribe-clear")).toContainText(
    /Unsubscribe and clear \d+ issues?/,
  );
  await dialog.getByRole("button", { name: "Keep it" }).click();
  await expect(dialog).toHaveCount(0);
});

/** Bring a hint back, as `mxr modes hint ID --show` does. */
async function showHint(page: Page, id: string) {
  await bridge(page, `/api/v1/mail/hints/${encodeURIComponent(id)}`, { seen: false });
}

async function hintSeen(page: Page, id: string): Promise<boolean> {
  const { guides } = await bridge<{ guides: { hints: { id: string; seen: boolean }[] }[] }>(
    page,
    "/api/v1/mail/modes/guide?mode=reading",
  );
  return guides[0]!.hints.find((hint) => hint.id === id)!.seen;
}

/** Move the cursor with j until `focused` holds it. */
async function cursorTo(page: Page, focused: ReturnType<Page["locator"]>) {
  for (let step = 0; step < 40; step += 1) {
    if ((await focused.count()) > 0) return;
    await page.keyboard.press("j");
  }
  throw new Error("the cursor never reached it");
}

test.describe("hints", () => {
  test.afterEach(async ({ page }) => {
    for (const id of ["reading.link", "reading.fading"]) {
      await bridge(page, `/api/v1/mail/hints/${id}`, { seen: true });
    }
  });

  test("no card at the top, and the link hint sits under the first link the cursor reaches", async ({
    page,
  }) => {
    await waitForEdition(page);
    await showHint(page, "reading.link");
    await openApp(page, "/reading");
    await expect(items(page).first()).toBeVisible();
    await expect(page.getByTestId("mode-card")).toHaveCount(0);
    await expect(page.getByTestId("hint")).toHaveCount(0);
    await cursorTo(page, page.locator("[data-testid='reading-link'][aria-current='true']"));
    const hint = page.locator("[data-hint='reading.link']");
    await expect(hint).toContainText("L fetches its article");
    await page.keyboard.press("Escape");
    await expect(hint).toHaveCount(0);
    await expect.poll(() => hintSeen(page, "reading.link")).toBe(true);
    // ? leads with the mode.
    await page.keyboard.press("?");
    await expect(page.getByTestId("mode-help")).toContainText("Reading:");
  });

  test("the fading hint sits under the Fading band and b dismisses it", async ({ page }) => {
    await waitForEdition(page);
    await showHint(page, "reading.fading");
    await openApp(page, "/reading");
    await expect(items(page).first()).toBeVisible();
    await cursorTo(
      page,
      page.locator("[data-testid='band-fading'] [data-testid='reading-item'][aria-current='true']"),
    );
    const hint = page.getByTestId("band-fading").locator("[data-hint='reading.fading']");
    await expect(hint).toContainText("b keeps one on Later");
    await page.keyboard.press("b");
    await expect(hint).toHaveCount(0);
    await expect.poll(() => hintSeen(page, "reading.fading")).toBe(true);
    await page.keyboard.press("u");
  });
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

  test("cards fill the width with no horizontal scroll, and the reader too", async ({ page }) => {
    await openEdition(page);
    const overflow = () =>
      page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
    expect(await overflow()).toBeLessThanOrEqual(0);
    await items(page).first().locator("h3 a").click();
    await expect(page.getByTestId("reading-reader")).toBeVisible();
    expect(await overflow()).toBeLessThanOrEqual(0);
  });
});

for (const colorScheme of ["dark", "light"] as const) {
  test.describe(`${colorScheme} scheme`, () => {
    test.use({ colorScheme });

    test(`axe: the edition, Later and the reader have no serious violations (${colorScheme})`, async ({
      page,
    }) => {
      await openEdition(page);
      await page.waitForLoadState("networkidle");
      expect(await blockingViolations(page)).toEqual([]);
      await openApp(page, "/reading?view=later");
      await page.waitForLoadState("networkidle");
      expect(await blockingViolations(page)).toEqual([]);
      const key = (await edition(page)).bands[0]!.items[0]!.item_key;
      await openApp(page, `/reading/item/${encodeURIComponent(key)}`);
      await expect(page.getByTestId("reading-reader")).toBeVisible();
      await page.waitForLoadState("networkidle");
      expect(await blockingViolations(page)).toEqual([]);
    });
  });
}
