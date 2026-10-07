import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface SubscriptionsAnswer {
  subscriptions: {
    subscriptions: { title: string; status: string }[];
  };
}

/** The demo's recurring receipts are filed and found as subscriptions. */
async function waitForSubscriptions(page: Page): Promise<void> {
  await expect
    .poll(
      async () => {
        const { subscriptions } = await bridge<SubscriptionsAnswer>(
          page,
          "/api/v1/mail/records/subscriptions",
        );
        return subscriptions.subscriptions.map((s) => `${s.title}: ${s.status}`);
      },
      { timeout: 60_000 },
    )
    .toEqual(
      expect.arrayContaining([
        "Spotify Premium: active",
        "Netflix: active",
        "Headspace: overdue",
        "1Password: active",
        "Disney+: ended",
      ]),
    );
}

async function openSubscriptions(page: Page): Promise<void> {
  await openApp(page, "/archive");
  await page.getByTestId("subscriptions-chip").click();
  await expect(page.getByTestId("subscriptions-chip")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("subscription-row").first()).toBeVisible();
}

const row = (page: Page, title: string) =>
  page.getByTestId("subscription-row").filter({ hasText: title });

test("Subscriptions shows one row per subscription with totals per currency", async ({ page }) => {
  await waitForSubscriptions(page);
  await openSubscriptions(page);
  const totals = page.getByTestId("subscription-totals");
  await expect(totals).toContainText(/£[\d,.]+ a month · £[\d,.]+ a year/);
  // Dollars are totalled on their own, never converted.
  await expect(totals).toContainText("$2.99 a month · $35.88 a year");
  await expect(row(page, "Spotify Premium")).toContainText("£11.99");
  await expect(row(page, "Spotify Premium")).toContainText(/next \d+ \w+ \d{4}/);
  // The Spotify gift card is a one-off, not a second subscription.
  await expect(page.getByTestId("subscription-row").filter({ hasText: "Spotify" })).toHaveCount(1);
  await expect(row(page, "Headspace")).toContainText("overdue since");
  await expect(row(page, "1Password")).toContainText("seen twice");
  await expect(page.getByRole("region", { name: "Ended" })).toContainText("Disney+");
  await expect(page.getByTestId("subscription-signals")).toContainText(
    "Netflix went up from £10.99 to £12.99",
  );
  await expect(page.getByTestId("subscription-signals")).toContainText("No Headspace charge since");
});

test("the card shows the price change, every charge and where the amount came from", async ({
  page,
}) => {
  await waitForSubscriptions(page);
  await openSubscriptions(page);
  await row(page, "Netflix").click();
  const card = page.getByTestId("subscription-card");
  await expect(card).toContainText("Netflix");
  await expect(card.getByTestId("price-changes")).toContainText("£10.99 to £12.99 on");
  await expect(card.getByTestId("subscription-charges").getByRole("listitem")).toHaveCount(4);
  await expect(card).toContainText("from a pattern in the email · unchecked");
});

test("keys move through subscriptions, p opens the issuer, Esc goes back to the ledger", async ({
  page,
}) => {
  await waitForSubscriptions(page);
  await openSubscriptions(page);
  await page.keyboard.press("Escape");
  const first = page.getByTestId("subscription-row").first();
  await row(page, "Headspace").click();
  await expect(row(page, "Headspace")).toHaveAttribute("aria-current", "true");
  await page.keyboard.press("j");
  await expect(row(page, "Headspace")).not.toHaveAttribute("aria-current", "true");
  await page.keyboard.press("k");
  await expect(row(page, "Headspace")).toHaveAttribute("aria-current", "true");
  await page.keyboard.press("p");
  await expect(page.getByTestId("issuer-page")).toContainText("Headspace");
  await expect(page.getByTestId("subscriptions-chip")).toHaveAttribute("aria-pressed", "false");
  await page.getByTestId("subscriptions-chip").click();
  await expect(first).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("subscriptions-chip")).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByTestId("record-row").first()).toBeVisible();
});

test("a price change in the coming-up strip opens its subscription", async ({ page }) => {
  await waitForSubscriptions(page);
  await openApp(page, "/archive");
  const signal = page.getByTestId("coming-up").getByRole("button", { name: /Netflix went up/ });
  await signal.click();
  await expect(page.getByTestId("subscriptions-chip")).toHaveAttribute("aria-pressed", "true");
  await expect(row(page, "Netflix")).toHaveAttribute("aria-current", "true");
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("subscriptions fit without sideways scroll and a row opens its card", async ({ page }) => {
    await waitForSubscriptions(page);
    await openSubscriptions(page);
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
    await row(page, "Spotify Premium").click();
    const screen = page.getByTestId("subscription-card-screen");
    await expect(screen.getByTestId("subscription-card")).toContainText("Spotify Premium");
    await screen.getByRole("button", { name: "Subscriptions" }).click();
    await expect(row(page, "Spotify Premium")).toBeVisible();
  });
});

for (const colorScheme of ["dark", "light"] as const) {
  test.describe(`${colorScheme} scheme`, () => {
    test.use({ colorScheme });

    test("axe: the subscriptions and a subscription's card", async ({ page }) => {
      await waitForSubscriptions(page);
      await openSubscriptions(page);
      await row(page, "Netflix").click();
      await expect(page.getByTestId("subscription-card")).toBeVisible();
      await page.waitForLoadState("networkidle");
      const results = await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa"])
        .exclude("iframe")
        .analyze();
      const blocking = results.violations
        .filter((violation) => violation.impact === "serious" || violation.impact === "critical")
        .flatMap((violation) =>
          violation.nodes.map(
            (node) => `${violation.id} (${violation.impact}): ${node.target.join(" ")}`,
          ),
        );
      expect(blocking).toEqual([]);
    });
  });
}
