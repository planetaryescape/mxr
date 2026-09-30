import { expect, test, type WebSocketRoute } from "@playwright/test";

import { openApp } from "./helpers/state";

test("event stream down > 30s with the daemon answering shows the live-updates banner", async ({
  page,
}) => {
  test.setTimeout(90_000);
  let streamUp = true;
  let live: WebSocketRoute | undefined;
  await page.routeWebSocket(/\/api\/v1\/events/, (ws) => {
    if (!streamUp) {
      void ws.close();
      return;
    }
    ws.connectToServer();
    live = ws;
  });
  await openApp(page, "/m/inbox");
  await expect(page.getByRole("contentinfo").getByText(/^connected$/i)).toBeVisible();

  // Drop the stream; every reconnect is refused from here on.
  streamUp = false;
  await live?.close();
  const banner = page.locator('[data-offline-banner="stream"]');
  await expect(banner).toBeVisible({ timeout: 35_000 });
  await expect(banner).toContainText("Live updates are paused");
  // The daemon still answers requests, so it isn't reported as stopped.
  await expect(page.locator('[data-offline-banner="daemon"]')).toHaveCount(0);

  streamUp = true;
  // Focus reconnects at once instead of waiting out the backoff.
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(banner).toBeHidden({ timeout: 10_000 });
});
