import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

import { cursorTo, mailList, openList, rowById } from "./helpers/mail";
import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

/*
 * Sorting shows its work (D119, rubric X14) on the demo mailbox: Now's
 * arrivals line sums, each count opens exactly that many emails, X moves
 * an email in every client at once, and undo puts it back.
 */

interface Count {
  bucket: string;
  count: number;
  label: string;
}

interface ArrivalsAnswer {
  arrivals: { since: string; until: string; total: number; counts: Count[]; also?: Count[] };
}

interface ListAnswer {
  list: { total: number; items: { message_id: string }[] };
}

async function line(page: Page): Promise<ArrivalsAnswer["arrivals"]> {
  return (await bridge<ArrivalsAnswer>(page, "/api/v1/mail/arrivals")).arrivals;
}

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

test("the arrivals line's counts sum, and each opens exactly that many emails", async ({
  page,
}) => {
  const arrivals = await line(page);
  expect(arrivals.counts.reduce((sum, count) => sum + count.count, 0)).toBe(arrivals.total);
  for (const count of arrivals.counts) {
    const params = new URLSearchParams({
      bucket: count.bucket,
      since: arrivals.since,
      until: arrivals.until,
    });
    const listed = await bridge<ListAnswer>(page, `/api/v1/mail/arrivals/list?${params}`);
    expect(listed.list.total, count.label).toBe(count.count);
  }

  await openApp(page, "/now");
  const shown = page.getByTestId("arrivals-line");
  await expect(shown).toBeVisible();
  const first = arrivals.counts[0];
  if (!first) {
    await expect(shown).toHaveText(/^(Nothing new since|Clear\.)/);
    return;
  }
  await shown.getByRole("link", { name: first.label }).click();
  await expect(page).toHaveURL(/\/arrivals\?/);
  await expect(page.getByTestId("arrival-row")).toHaveCount(Math.min(first.count, 500));
});

test("X moves an email, every client sees it, and u puts it back", async ({ page, browser }) => {
  // A second client watching Inbox: the move reaches it through the daemon.
  const other = await browser.newPage();
  await openList(other, "/m/inbox");

  await openList(page, "/m/inbox");
  await mailList(page).focus();
  const rowId = await cursorTo(page, () => true);
  const chip = rowById(page, rowId).getByTestId("mode-chip");
  await expect(chip).toBeVisible();
  const before = (await chip.textContent()) ?? "";
  const [key, after] = before === "Reading" ? ["u", "Updates"] : ["r", "Reading"];

  await page.keyboard.press("X");
  const picker = page.getByTestId("move-to-mode-dialog");
  await expect(picker).toBeVisible();
  await page.keyboard.press(key);
  await expect(picker).toHaveCount(0);
  await expect(page.locator("[data-sonner-toast]").filter({ hasText: /^Moved to / })).toBeVisible();
  await expect(chip).toHaveText(after);
  await expect(chip).toHaveAttribute("title", /you moved this email/);
  await expect(rowById(other, rowId).getByTestId("mode-chip")).toHaveText(after);

  await page.keyboard.press("u");
  await expect(chip).toHaveText(before);
  await expect(rowById(other, rowId).getByTestId("mode-chip")).toHaveText(before);
  await other.close();
});

test("the reader's expand-all moved to ; so X can move the email", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  await page.keyboard.press("X");
  await expect(page.getByTestId("move-to-mode-dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("move-to-mode-dialog")).toHaveCount(0);
});

for (const colorScheme of ["dark", "light"] as const) {
  test.describe(`${colorScheme} scheme`, () => {
    test.use({ colorScheme });

    test("axe: Now with its arrivals line", async ({ page }) => {
      await openApp(page, "/now");
      await expect(page.getByTestId("arrivals-line")).toBeVisible();
      await page.waitForLoadState("networkidle");
      expect(await blockingViolations(page)).toEqual([]);
    });

    test("axe: an arrivals list", async ({ page }) => {
      const arrivals = await line(page);
      const params = new URLSearchParams({ since: arrivals.since, until: arrivals.until });
      await openApp(page, `/arrivals?${params}`);
      await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
      await page.waitForLoadState("networkidle");
      expect(await blockingViolations(page)).toEqual([]);
    });

    test("axe: the move picker", async ({ page }) => {
      await openList(page, "/m/inbox");
      await page.keyboard.press("X");
      await expect(page.getByTestId("move-to-mode-dialog")).toBeVisible();
      expect(await blockingViolations(page)).toEqual([]);
    });
  });
}
