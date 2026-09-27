import { expect, test, type Page } from "@playwright/test";

import { mailList, mailRows, modKey, openList, reader } from "./helpers/mail";

test.use({ viewport: { width: 1440, height: 900 } });

// The demo mail has no quoted replies or signatures, so these specs give the
// newest message of the opened conversation a real-world reply body. The
// daemon's response shape is kept; only the text changes.
const QUOTED_PLAIN = [
  "Sounds good, let's ship on Thursday.",
  "",
  "-- ",
  "Ada Lovelace",
  "Engines Ltd",
  "",
  "On Mon, Sep 21, 2026 at 10:02 AM Grace Hopper <grace@example.com> wrote:",
  "> OLD-QUOTED-TEXT one",
  "> two",
  "> three",
  "> four",
  "> five",
].join("\n");
const QUOTED_HTML =
  `<div dir="ltr">Sounds good, let's ship on Thursday.</div>` +
  `<div class="gmail_quote"><div class="gmail_attr">On Mon Grace wrote:</div>` +
  `<blockquote>OLD-QUOTED-TEXT</blockquote></div>`;

async function stubNewestBody(page: Page): Promise<void> {
  await page.route("**/api/v1/mail/threads/**", async (route) => {
    const response = await route.fetch();
    const json = await response.json();
    const newest = json.bodies?.at(-1);
    if (newest) {
      newest.text_plain = QUOTED_PLAIN;
      newest.text_html = QUOTED_HTML;
    }
    await route.fulfill({ response, json });
  });
}

async function openFirstConversation(page: Page): Promise<void> {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  await expect(reader(page)).toBeVisible();
}

test("plain text folds the quote and the signature until asked", async ({ page }) => {
  await stubNewestBody(page);
  await openFirstConversation(page);
  // H toggles Formatted off again, back to plain text (TUI toggle).
  await expect(reader(page).getByRole("radio", { name: "Formatted" })).toBeChecked();
  await page.keyboard.press("H");
  await expect(reader(page).getByRole("radio", { name: "Plain" })).toBeChecked();

  const newest = reader(page).getByTestId("thread-message").last();
  await expect(newest.getByText("Sounds good, let's ship on Thursday.")).toBeVisible();
  await expect(newest.getByText(/OLD-QUOTED-TEXT/)).toHaveCount(0);
  await expect(newest.getByText("Engines Ltd")).toHaveCount(0);

  await newest.getByRole("button", { name: /quoted lines?/ }).click();
  await expect(newest.getByText(/OLD-QUOTED-TEXT/)).toBeVisible();
  await newest.getByRole("button", { name: "Signature" }).click();
  await expect(newest.getByText("Engines Ltd")).toBeVisible();
});

test("formatted HTML hides the quote behind Show quoted text", async ({ page }) => {
  await stubNewestBody(page);
  await openFirstConversation(page);
  const newest = reader(page).getByTestId("thread-message").last();
  const body = newest.frameLocator('iframe[title="HTML message body"]');
  await expect(body.getByText("Sounds good, let's ship on Thursday.")).toBeVisible();
  await expect(body.getByText("OLD-QUOTED-TEXT")).toHaveCount(0);

  await newest.getByRole("button", { name: "Show quoted text" }).click();
  await expect(body.getByText("OLD-QUOTED-TEXT")).toBeVisible();
});

test("R and H toggle their view, and again return to plain, as in the TUI", async ({ page }) => {
  await openFirstConversation(page);
  const radio = (name: string) => reader(page).getByRole("radio", { name });
  await page.keyboard.press("R");
  await expect(radio("Reader")).toBeChecked();
  await page.keyboard.press("R");
  await expect(radio("Plain")).toBeChecked();
  await page.keyboard.press("H");
  await expect(radio("Formatted")).toBeChecked();
  await page.keyboard.press("H");
  await expect(radio("Plain")).toBeChecked();
});

test("L lists the conversation's links; Enter opens one and y copies it", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await openList(page, "/m/inbox");
  // A conversation whose body carries links (most demo mail does).
  for (let step = 0; step < 12; step += 1) {
    await page.keyboard.press("Enter");
    await expect(reader(page)).toBeVisible();
    await page.keyboard.press("L");
    const dialog = page.getByRole("dialog", { name: "Links" });
    await expect(dialog).toBeVisible();
    const options = dialog.getByRole("option");
    await expect
      .poll(async () => (await options.count()) + (await dialog.getByText("no links").count()))
      .toBeGreaterThan(0);
    if ((await options.count()) === 0) {
      await page.keyboard.press("Escape");
      await page.keyboard.press("n");
      continue;
    }
    const href = (await options.first().locator(".font-mono").innerText()).trim();
    await page.keyboard.press("y");
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(href);

    // Links open in a new tab with noopener; record the call rather than
    // depend on a demo host that doesn't resolve.
    await page.evaluate(() => {
      const opened: string[] = [];
      (window as unknown as { __opened: string[] }).__opened = opened;
      window.open = ((url?: string | URL) => {
        opened.push(String(url));
        return null;
      }) as typeof window.open;
    });
    await page.keyboard.press("Enter");
    await expect
      .poll(() => page.evaluate(() => (window as unknown as { __opened: string[] }).__opened))
      .toEqual([href]);
    return;
  }
  throw new Error("no conversation with links in the first twelve");
});

test("the list toggles between conversations and single messages", async ({ page }) => {
  await openList(page, "/m/inbox");
  const threaded = mailRows(page).filter({ hasText: /./ });
  await expect(
    mailList(page).getByRole("option", { name: /messages in conversation/ }).first(),
  ).toBeVisible();

  await page.keyboard.press(`${await modKey(page)}+k`);
  await page.keyboard.type("Toggle threads");
  await page.keyboard.press("Enter");

  // Message mode: no row stands for a conversation.
  await expect(
    mailList(page).getByRole("option", { name: /messages in conversation/ }),
  ).toHaveCount(0);
  await expect(threaded.first()).toBeVisible();

  await page.keyboard.press(`${await modKey(page)}+k`);
  await page.keyboard.type("Toggle threads");
  await page.keyboard.press("Enter");
  await expect(
    mailList(page).getByRole("option", { name: /messages in conversation/ }).first(),
  ).toBeVisible();
});

test("reader text keeps a readable measure in a wide window", async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 1000 });
  await openFirstConversation(page);
  await page.keyboard.press("R");
  const newest = reader(page).getByTestId("thread-message").last();
  const perLine = await newest.evaluate((node) => {
    const paragraph = [...node.querySelectorAll<HTMLElement>(".whitespace-pre-wrap")].find(
      (el) => (el.textContent ?? "").length > 0,
    );
    if (!paragraph) return 0;
    const probe = document.createElement("span");
    probe.textContent = "abcdefghijklmnopqrstuvwxyz".repeat(4);
    probe.style.whiteSpace = "nowrap";
    paragraph.append(probe);
    const charWidth = probe.getBoundingClientRect().width / probe.textContent.length;
    probe.remove();
    return paragraph.getBoundingClientRect().width / charWidth;
  });
  // Rubric 2.6: 60 to 80 characters, with a little slack for proportional type.
  expect(perLine).toBeGreaterThan(55);
  expect(perLine).toBeLessThan(90);
});
