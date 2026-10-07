import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

import { deliver, failSyncs, restoreSyncs } from "./helpers/spool";
import { bridge, bridgeAuth, openApp, readE2EState } from "./helpers/state";

/*
 * The freshness indicator: when the newest mail arrived and how sync is
 * doing, on every page, with the last arrivals one click away. Mail and
 * sync failures come from the fake provider's spool.
 */

interface FreshnessReply {
  freshness: {
    newest_message_at?: string | null;
    arrivals: { message_id: string; thread_id: string; subject: string }[];
  };
}

const shotsDir = process.env.MXR_FRESHNESS_SHOTS;

async function shot(page: Page, name: string) {
  if (!shotsDir) return;
  // Let overlays finish fading in.
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${shotsDir}/${name}.png` });
}

/** Run a sync through the bridge and wait for it to finish, failed or not. */
async function syncNow(page: Page) {
  await page.request.post(`${readE2EState().bridgeUrl}/api/v1/mail/sync`, {
    headers: bridgeAuth(),
  });
}

function statusBar(page: Page) {
  return page.getByRole("contentinfo");
}

/** The visible indicator: the status bar's, or the topbar's on a phone. */
function trigger(page: Page) {
  return page.getByTestId("freshness-trigger").filter({ visible: true });
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

test.describe("desktop", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("the status bar shows when the newest mail arrived, with the exact time on hover", async ({
    page,
  }) => {
    await openApp(page, "/now");
    const reply = await bridge<FreshnessReply>(page, "/api/v1/mail/freshness");
    const newest = reply.freshness.newest_message_at;
    expect(newest).toBeTruthy();
    const minutes = Math.floor((Date.now() - new Date(newest ?? "").getTime()) / 60_000);
    const expected =
      minutes < 1
        ? "just now"
        : minutes < 60
          ? `${minutes}m ago`
          : minutes < 1440
            ? `${Math.floor(minutes / 60)}h ago`
            : `${Math.floor(minutes / 1440)}d ago`;
    await expect(trigger(page)).toHaveText(`Latest mail ${expected}`);
    await expect(statusBar(page).getByText(/^synced (just now|\d+m ago)$/)).toBeVisible();

    await trigger(page).hover();
    // Formatted in the browser, whose locale the app uses.
    const exact = await page.evaluate(
      (iso) =>
        new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(
          new Date(iso),
        ),
      newest ?? "",
    );
    await expect(page.getByRole("tooltip")).toContainText(exact);
  });

  test("the popover lists the last arrivals with their modes and opens one", async ({ page }) => {
    await openApp(page, "/now");
    await trigger(page).click();
    const arrivals = page.getByTestId("freshness-arrival");
    await expect(arrivals.first()).toBeVisible();
    expect(await arrivals.count()).toBeLessThanOrEqual(5);
    await expect(
      page
        .getByTestId("freshness-arrival")
        .filter({ hasText: /→ (Messages|To do|Updates|Reading|Archive) · [a-z-]+/ })
        .first(),
    ).toBeVisible();
    await shot(page, "desktop-popover");

    const reply = await bridge<FreshnessReply>(page, "/api/v1/mail/freshness");
    const first = reply.freshness.arrivals[0];
    await arrivals.first().click();
    await expect(page).toHaveURL(new RegExp(`/m/inbox/${first?.thread_id}`));
    await expect(page.getByRole("article", { name: /^Conversation:/ })).toBeVisible();
  });

  test("a new message updates the indicator and the popover live", async ({ page }) => {
    await openApp(page, "/todo");
    await expect(trigger(page)).toBeVisible();
    await trigger(page).click();
    const subject = `Freshness check ${Date.now()}`;
    // Out of the inbox on arrival, so it changes no other spec's lists.
    deliver({
      from: "Status Robot <status@robots.example>",
      to: "alex@demo.mxr.local",
      subject,
      body: "All systems normal.",
      labels: ["UNREAD"],
    });
    await syncNow(page);

    const newest = page.getByTestId("freshness-arrival").first();
    await expect(newest).toContainText("Status Robot");
    await expect(newest).toContainText("→ out of the inbox");
    await expect(newest).toContainText(subject);
    await page.keyboard.press("Escape");
    await expect(trigger(page)).toHaveText("Latest mail just now");
  });

  test("a failing sync turns into a warning that links to the sync details", async ({ page }) => {
    await openApp(page, "/now");
    await expect(trigger(page)).toBeVisible();
    try {
      failSyncs("connection refused");
      await syncNow(page);
      const warning = page.getByTestId("freshness-warning");
      await expect(warning).toHaveText(
        /^Fake Account unreachable, (last sync (just now|\d+m ago)|retrying .+)$/,
      );
      await expect(warning).toHaveAttribute("href", /^\/accounts\/[^/]+$/);
      await expect(trigger(page).locator("[data-tone]")).toHaveAttribute("data-tone", "bad");
      await shot(page, "desktop-warning");

      await trigger(page).click();
      await expect(page.getByRole("dialog", { name: "Last arrivals" })).toContainText(
        "Fake Account unreachable",
      );
      await page.keyboard.press("Escape");
      await warning.click();
      await expect(page).toHaveURL(/\/accounts\/[^/]+$/);
      const details = page.getByTestId("account-sync-details");
      await expect(details).toContainText(/Can't sync/);
      await expect(details).toContainText("Can't reach Fake Account.");
      await expect(page.getByRole("status").filter({ hasText: /Can't sync/ })).toBeVisible();
      await shot(page, "desktop-account-sync");

      await page.goto("/accounts");
      await expect(page.getByTestId("account-sync-health").first()).toHaveText(/^Can't sync/);
    } finally {
      restoreSyncs();
      await syncNow(page);
    }
    await expect(page.getByTestId("freshness-warning")).toBeHidden();
    await expect(statusBar(page).getByText(/^synced just now$/)).toBeVisible();
  });
});

test.describe("phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

  test("only the age and a health dot show, and a tap opens the arrivals", async ({ page }) => {
    await openApp(page, "/now");
    const compact = trigger(page);
    await expect(compact).toBeVisible();
    await expect(compact).toHaveText(/^(just now|\d+[mhd] ago)$/);
    await expect(compact.locator("[data-tone]")).toHaveAttribute("data-tone", "ok");
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBe(0);
    const box = await compact.boundingBox();
    expect(box?.height ?? 0).toBeGreaterThanOrEqual(32);
    await shot(page, "phone-bar");

    await compact.tap();
    await expect(page.getByTestId("freshness-arrival").first()).toBeVisible();
    const popover = await page.getByRole("dialog", { name: "Last arrivals" }).boundingBox();
    expect((popover?.x ?? -1) >= 0 && (popover?.x ?? 0) + (popover?.width ?? 0) <= 390).toBe(true);
    await shot(page, "phone-popover");
  });
});

for (const colorScheme of ["dark", "light"] as const) {
  test.describe(`${colorScheme} scheme`, () => {
    test.use({ colorScheme, viewport: { width: 1440, height: 900 } });

    test("axe: the indicator, its warning and the open popover pass", async ({ page }) => {
      await openApp(page, "/now");
      await expect(trigger(page)).toBeVisible();
      await trigger(page).click();
      await expect(page.getByTestId("freshness-arrival").first()).toBeVisible();
      expect(await blockingViolations(page)).toEqual([]);
      await shot(page, `${colorScheme}-popover`);
      await page.keyboard.press("Escape");
      try {
        failSyncs("rate_limit 120");
        await syncNow(page);
        await expect(page.getByTestId("freshness-warning")).toContainText("paused: rate limited");
        // The account turned unhealthy: one toast says so. Let it finish
        // sliding in so axe reads its resting colours.
        const toast = page.locator("[data-sonner-toast]").filter({ hasText: "can't sync" });
        await expect(toast).toHaveCount(1);
        await expect(toast).toHaveAttribute("data-mounted", "true");
        await page.waitForTimeout(600);
        expect(await blockingViolations(page)).toEqual([]);
        await shot(page, `${colorScheme}-paused`);
      } finally {
        restoreSyncs();
        await syncNow(page);
      }
    });
  });
}
