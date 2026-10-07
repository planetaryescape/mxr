import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

import { mailList, openList } from "./helpers/mail";
import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface LedgerAnswer {
  ledger: {
    total: number;
    records: { issuer?: string | null; title?: string | null }[];
  };
}

/** The demo's records are filed: the Dell order and the Lisbon flight. */
async function waitForRecords(page: Page): Promise<number> {
  let total = 0;
  await expect
    .poll(
      async () => {
        const { ledger } = await bridge<LedgerAnswer>(page, "/api/v1/mail/records");
        total = ledger.total;
        return ledger.records.map((record) => record.title ?? "");
      },
      { timeout: 60_000 },
    )
    .toEqual(expect.arrayContaining(["XPS 14 laptop", "LHR -> LIS TP1357"]));
  return total;
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

const ask = (page: Page) => page.getByTestId("archive-ask");

/** The answer box has the focus on arrival; Esc hands the keys to the ledger. */
async function leaveAskBox(page: Page): Promise<void> {
  await expect(ask(page)).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(ask(page)).not.toBeFocused();
}

test("the ledger is one row per record, by month with its count and total", async ({ page }) => {
  await waitForRecords(page);
  await openApp(page, "/archive");
  await expect(page.getByTestId("mode-header")).toHaveText(
    "Receipts, orders, bookings and documents. Ask for what you need.",
  );
  const month = page.getByTestId("record-month").first();
  await expect(month.getByTestId("month-totals")).toHaveText(/^\d+ · (£|€|no amounts)/);
  // The Dell order's three emails are one row, with its stages under it.
  const dell = page.getByTestId("record-row").filter({ hasText: "XPS 14 laptop" });
  await expect(dell).toHaveCount(1);
  await expect(dell).toContainText("£1,249.00");
  await expect(dell).toContainText("ordered · shipped · delivered");
  // Archive is built: no early-version note.
  await expect(page.getByTestId("early-version")).toHaveCount(0);
  // The Lisbon trip, three days away, is coming up.
  await expect(page.getByTestId("coming-up")).toContainText(/Lisbon, .* · in 3 days/);
});

test("the answer box returns the booking reference with where it came from, and y copies it", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await waitForRecords(page);
  await openApp(page, "/archive");
  // The answer box has the focus on arrival.
  await expect(ask(page)).toBeFocused();
  await page.keyboard.type("lisbon booking ref");
  const card = page.getByTestId("answer-card");
  await expect(card.getByTestId("answer-value")).toHaveText("K7QX2M");
  await expect(card).toContainText("Booking ref");
  await expect(card).toContainText("from schema.org markup · checked");
  await expect(card).toContainText(/Part of trip "Lisbon, .*" \(3\)/);
  await page.keyboard.press("Enter");
  await page.keyboard.press("y");
  await expect(page.locator("[data-sonner-toast]").first()).toContainText("K7QX2M copied");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("K7QX2M");
});

test("a query no record matches says so before searching all mail", async ({ page }) => {
  await waitForRecords(page);
  await openApp(page, "/archive");
  await ask(page).fill("boiler warranty");
  // Typing matches record fields only; the model is never asked per key.
  await expect(page.getByTestId("answer-fallback")).toContainText(
    'No record matches "boiler warranty".',
  );
  await expect(page.getByTestId("answer-fallback")).toContainText(
    "Press Enter to search all mail.",
  );
  await ask(page).press("Enter");
  await expect(page.getByTestId("answer-fallback")).toContainText(
    'No record matches "boiler warranty". Searching all mail instead.',
  );
});

test("a broad issuer query lists every match in place of the ledger, and Clear brings it back", async ({
  page,
}) => {
  const total = await waitForRecords(page);
  await openApp(page, "/archive");
  await ask(page).fill("octopus");
  const matches = page.getByTestId("answer-list");
  await expect(matches.getByTestId("answer-list-header")).toHaveText(
    /^Octopus Energy · 3 records · £/,
  );
  await expect(matches.getByTestId("answer-list-span")).toBeVisible();
  // Every bill, not just the newest: no answer card hides the rest.
  await expect(page.getByTestId("answer-card")).toHaveCount(0);
  const rows = page.getByTestId("record-row");
  await expect(rows).toHaveCount(3);
  for (const row of await rows.all()) await expect(row).toContainText("Octopus Energy");
  await expect(page.locator('[data-testid="record-row"][data-best="true"]')).toHaveCount(1);
  // The ledger's own chips don't narrow a list.
  await expect(page.getByRole("group", { name: "Kind" })).toHaveCount(0);
  await matches.getByRole("button", { name: "Clear search" }).click();
  await expect(matches).toHaveCount(0);
  await expect(ask(page)).toHaveValue("");
  await expect(ask(page)).toBeFocused();
  await expect(rows).toHaveCount(total);
});

test("Show all turns an answer into the list of its matches, best one marked", async ({ page }) => {
  await waitForRecords(page);
  await openApp(page, "/archive");
  await ask(page).fill("lisbon booking ref");
  const card = page.getByTestId("answer-card");
  await expect(card.getByTestId("answer-value")).toHaveText("K7QX2M");
  const showAll = card.getByTestId("answer-show-all");
  await expect(showAll).toHaveText(/^Show all \d+ matches$/);
  const count = Number((await showAll.textContent())?.match(/\d+/)?.[0]);
  expect(count).toBeGreaterThan(1);
  await showAll.click();
  await expect(page.getByTestId("answer-list-header")).toContainText(`${count} records`);
  await expect(page.getByTestId("record-row")).toHaveCount(count);
  await expect(page.locator('[data-testid="record-row"][data-best="true"]')).toContainText(
    "LHR -> LIS TP1357",
  );
  // A new query decides for itself again.
  await ask(page).fill("dell");
  await expect(page.getByTestId("answer-card")).toContainText("Dell");
  await expect(page.getByTestId("answer-list")).toHaveCount(0);
});

test("E previews the export's rows, unchecked rows and missing PDFs, then downloads that CSV", async ({
  page,
}) => {
  const total = await waitForRecords(page);
  await openApp(page, "/archive");
  await leaveAskBox(page);
  await page.keyboard.press("E");
  const dialog = page.getByTestId("export-dialog");
  await expect(dialog.getByTestId("export-rows")).toHaveText(String(total));
  await expect(dialog.getByTestId("export-unchecked")).toHaveText(/^\d+$/);
  await expect(dialog.getByTestId("export-missing")).toHaveText(/^\d+$/);
  await expect(dialog.getByTestId("export-preview")).toContainText("unchecked amount or date");
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    dialog.getByRole("button", { name: /^Download CSV/ }).click(),
  ]);
  expect(download.suggestedFilename()).toBe("mxr-records.csv");
  const path = await download.path();
  const { readFileSync } = await import("node:fs");
  const lines = readFileSync(path, "utf8").trim().split("\n");
  expect(lines[0]).toMatch(/^date,kind,issuer,what,amount,currency,reference,checked/);
  expect(lines).toHaveLength(total + 1);
});

test("p opens the issuer's page and [ steps back a year", async ({ page }) => {
  await waitForRecords(page);
  await openApp(page, "/archive");
  await ask(page).fill("dell");
  await expect(page.getByTestId("answer-card")).toContainText("Dell");
  await page.keyboard.press("Enter");
  await page.keyboard.press("p");
  await expect(page.getByTestId("issuer-page")).toContainText("Dell");
  await expect(page.getByTestId("record-row")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("issuer-page")).toHaveCount(0);
  await page.keyboard.press("[");
  await expect(page.getByRole("button", { name: /^Clear \d{4}$/ })).toBeVisible();
});

test("X takes a record out of Archive without touching the email, and u brings it back", async ({
  page,
}) => {
  await waitForRecords(page);
  await openApp(page, "/archive");
  await ask(page).fill("apple");
  await expect(page.getByTestId("answer-card")).toContainText("Apple");
  await page.keyboard.press("Enter");
  await page.keyboard.press("X");
  const toast = page.locator("[data-sonner-toast]").first();
  await expect(toast).toContainText("Not a record. Removed from Archive; the email is untouched.");
  await expect(toast).not.toContainText("Archived");
  await expect(page.getByTestId("record-row").filter({ hasText: "Apple" })).toHaveCount(0);
  await page.keyboard.press("u");
  await expect(page.getByTestId("record-row").filter({ hasText: "Apple" })).toHaveCount(1);
});

test("T on a conversation previews its record card before filing it in Archive", async ({
  page,
}) => {
  await openList(page, "/m/inbox");
  await mailList(page).focus();
  await page.keyboard.press("T");
  const dialog = page.getByTestId("pass-to-mode-dialog");
  await expect(dialog).toContainText("Archive: file it as a record.");
  await expect(dialog.getByTestId("file-preview")).toBeVisible();
  await expect(dialog.getByTestId("file-preview")).toContainText("Would file in Archive");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toHaveCount(0);
});

test("the first answer carries its hint; dismissed, it never returns", async ({ page }) => {
  await waitForRecords(page);
  await bridge(page, "/api/v1/mail/hints/archive.answer", { seen: false });
  await openApp(page, "/archive");
  await expect(page.getByTestId("mode-card")).toHaveCount(0);
  await expect(page.getByTestId("hint")).toHaveCount(0);
  // Typing is a use of the page; fill() alone sends no key events.
  await ask(page).pressSequentially("lisbon booking ref");
  await expect(page.getByTestId("answer-value")).toHaveText("K7QX2M");
  const hint = page.getByTestId("hint");
  await expect(hint).toHaveText(/y copies what this answer found; Enter opens the document\./);
  await expect(hint).toHaveCount(1);
  await hint.getByRole("button", { name: "Dismiss hint (Esc)" }).click();
  await expect(page.getByTestId("hint")).toHaveCount(0);
  await page.reload();
  await ask(page).pressSequentially("lisbon booking ref");
  await expect(page.getByTestId("answer-value")).toHaveText("K7QX2M");
  await expect(page.getByTestId("hint")).toHaveCount(0);
});

test("? leads with what Archive is for", async ({ page }) => {
  await waitForRecords(page);
  await openApp(page, "/archive");
  await leaveAskBox(page);
  await page.keyboard.press("?");
  await expect(page.getByTestId("mode-help")).toContainText(
    "Archive: Receipts, orders, bookings and documents.",
  );
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("the ledger fits without sideways scroll and a row opens its card full screen", async ({
    page,
  }) => {
    await waitForRecords(page);
    await openApp(page, "/archive");
    await expect(page.getByTestId("record-row").first()).toBeVisible();
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
    await page.getByTestId("record-row").filter({ hasText: "XPS 14 laptop" }).click();
    const screen = page.getByTestId("record-card-screen");
    await expect(screen.getByTestId("record-card")).toContainText("Dell");
    await expect(screen.getByTestId("record-pdf")).toContainText("Invoice-402118.pdf");
    const cardOverflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(cardOverflow).toBeLessThanOrEqual(0);
  });
});

for (const colorScheme of ["dark", "light"] as const) {
  test.describe(`${colorScheme} scheme`, () => {
    test.use({ colorScheme });

    test("axe: the answer card and record card", async ({ page }) => {
      await waitForRecords(page);
      await openApp(page, "/archive");
      await ask(page).fill("lisbon booking ref");
      await expect(page.getByTestId("answer-value")).toHaveText("K7QX2M");
      await expect(page.getByTestId("record-card")).toBeVisible();
      await page.waitForLoadState("networkidle");
      expect(await blockingViolations(page)).toEqual([]);
    });

    test("axe: a listed query", async ({ page }) => {
      await waitForRecords(page);
      await openApp(page, "/archive");
      await ask(page).fill("octopus");
      await expect(page.getByTestId("answer-list")).toBeVisible();
      await expect(page.getByTestId("record-row")).toHaveCount(3);
      await page.waitForLoadState("networkidle");
      expect(await blockingViolations(page)).toEqual([]);
    });

    test("axe: the export preview", async ({ page }) => {
      await waitForRecords(page);
      await openApp(page, "/archive");
      await leaveAskBox(page);
      await page.keyboard.press("E");
      await expect(page.getByTestId("export-preview")).toBeVisible();
      expect(await blockingViolations(page)).toEqual([]);
    });
  });
}
