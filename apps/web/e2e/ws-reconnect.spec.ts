import { expect, test } from "@playwright/test";

import { openApp, restartDaemon, stopDaemon } from "./helpers/state";

test("a stopped local app is distinguished from an event reconnect and recovers", async ({
  page,
}) => {
  await openApp(page, "/m/inbox");
  const connectionPill = page.getByRole("contentinfo").getByText(/^connected$/i);
  await expect(connectionPill).toBeVisible();

  await stopDaemon();
  try {
    await expect(
      page.getByRole("contentinfo").getByText("local app stopped", { exact: true }),
    ).toBeVisible({
      timeout: 10_000,
    });
  } finally {
    await restartDaemon();
  }
  await expect(connectionPill).toBeVisible({ timeout: 30_000 });
});
