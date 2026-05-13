import { expect, test } from "@playwright/test";

import { openApp, readE2EState } from "./helpers/state";

test("app shell renders against a real fake-provider daemon", async ({ page, request }) => {
  const state = readE2EState();
  const status = await request.get(`${state.bridgeUrl}/api/v1/admin/status`, {
    headers: { authorization: `Bearer ${state.token}` },
  });
  expect(status.ok()).toBe(true);

  await openApp(page);
  // The router redirects / to /m/inbox.
  await expect(page).toHaveURL(/\/m\/inbox$/);
  // Sidebar and topbar are rendered with live bridge data.
  await expect(page.getByRole("button", { name: /compose/i })).toBeVisible();
  await expect(page.getByText(/Fake Account|Demo/i).first()).toBeVisible();
});
