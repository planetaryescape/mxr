import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

import { openApp } from "./helpers/state";

const ROUTES = ["/m/inbox", "/compose/new", "/search?q=test", "/settings/theme", "/onboarding"];

for (const path of ROUTES) {
  test(`axe-core: no violations on ${path}`, async ({ page }) => {
    await openApp(page, path);
    const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
    expect(results.violations).toEqual([]);
  });
}

test("every visible button has an accessible name", async ({ page }) => {
  await openApp(page, "/m/inbox");
  const buttons = await page.getByRole("button").all();
  for (const btn of buttons) {
    if (!(await btn.isVisible())) continue;
    const label =
      (await btn.getAttribute("aria-label"))?.trim() ?? (await btn.textContent())?.trim();
    expect(label, `button without accessible name: ${await btn.innerHTML()}`).toBeTruthy();
  }
});
