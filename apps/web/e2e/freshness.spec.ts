import { expect, test, type Page } from "@playwright/test";

import { deliver, failSyncs, restoreSyncs } from "./helpers/spool";
import { bridge, bridgeAuth, openApp, readE2EState } from "./helpers/state";
import type { Arrival } from "../src/features/freshness/copy";

/*
 * The freshness indicator: when the newest mail arrived and how sync is
 * doing, on every page, with the last arrivals one click away. Mail and
 * sync failures come from the fake provider's spool.
 */

interface FreshnessReply {
  freshness: {
    newest_message_at?: string | null;
    arrivals: Arrival[];
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

test.describe("desktop", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("a new message updates the indicator and the popover live", async ({ page }) => {
    // A page with no lit row, so the status bar keeps its full freshness words.
    await openApp(page, "/rules");
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
    const observed = (
      await bridge<FreshnessReply>(page, "/api/v1/mail/freshness")
    ).freshness.arrivals.find((arrival) => arrival.subject === subject);
    expect(observed?.in_inbox).toBe(false);
    expect(observed?.modes.length).toBeGreaterThan(0);
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
        /^Can't reach alex@demo\.mxr\.local, (last sync (just now|\d+m ago)|retrying .+)$/,
      );
      await expect(warning).toHaveAttribute("href", /^\/accounts\/[^/]+$/);
      await expect(trigger(page).locator("[data-tone]")).toHaveAttribute("data-tone", "bad");
      await shot(page, "desktop-warning");

      await trigger(page).click();
      await expect(page.getByRole("dialog", { name: "Last arrivals" })).toContainText(
        "Can't reach alex@demo.mxr.local",
      );
      await page.keyboard.press("Escape");
      await warning.click();
      await expect(page).toHaveURL(/\/accounts\/[^/]+$/);
      const details = page.getByTestId("account-sync-details");
      await expect(details).toContainText(/Can't sync/);
      await expect(details).toContainText("Can't reach alex@demo.mxr.local.");
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
