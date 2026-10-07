import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

import { mailRows, openList, pressSequence, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

/** Serious and critical axe violations, one line per node. */
async function blockingViolations(page: Page): Promise<string[]> {
  // Message bodies render in sandboxed srcdoc iframes that forbid scripts, so
  // axe cannot run inside them; they hold untrusted email, not app UI.
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

const ROUTES = [
  "/now",
  "/messages",
  "/messages?turn=theirs",
  "/messages?person=person%3Asamir%40launchpad.example",
  "/updates",
  "/reading",
  "/archive",
  "/find",
  "/desk",
  "/desk?lane=waiting",
  "/todo",
  "/todo?view=catchup",
  "/todo?view=expired",
  "/m/inbox",
  "/m/label/work",
  "/search?q=canary",
  "/snoozed",
  "/drafts",
  "/rules",
  "/settings/theme",
  "/onboarding",
];

// No theme is saved, so the app follows the browser's scheme: dark gives
// midnight, light gives light. Both ship as defaults, so both are checked.
for (const colorScheme of ["dark", "light"] as const) {
  test.describe(`${colorScheme} scheme`, () => {
    test.use({ colorScheme });

    for (const path of ROUTES) {
      test(`axe: no serious or critical violations on ${path}`, async ({ page }) => {
        await openApp(page, path);
        await expect(page.locator("#main")).toBeVisible();
        await page.waitForLoadState("networkidle");
        expect(await blockingViolations(page)).toEqual([]);
      });
    }

    test("axe: open thread", async ({ page }) => {
      await openList(page, "/m/inbox");
      await page.keyboard.press("Enter");
      await expect(reader(page)).toBeVisible();
      await page.waitForLoadState("networkidle");
      expect(await blockingViolations(page)).toEqual([]);
    });

    test("axe: labels dialog", async ({ page }) => {
      await openList(page, "/m/inbox");
      await page.keyboard.press("l");
      await expect(page.getByRole("dialog", { name: "Labels" })).toBeVisible();
      expect(await blockingViolations(page)).toEqual([]);
    });

    test("axe: keyboard help", async ({ page }) => {
      await openList(page, "/m/inbox");
      await page.keyboard.press("?");
      await expect(page.getByRole("dialog", { name: "Keyboard" })).toBeVisible();
      expect(await blockingViolations(page)).toEqual([]);
    });

    test("axe: compose overlay", async ({ page }) => {
      await openList(page, "/m/inbox");
      await page.keyboard.press("c");
      const composer = page.getByRole("dialog", { name: "New message" });
      // Let the lazy CodeMirror editor mount and label itself before scanning.
      await expect(
        composer.getByRole("group", { name: "Message body" }).locator(".cm-content"),
      ).toBeVisible();
      expect(await blockingViolations(page)).toEqual([]);
    });
  });
}

test("every visible button has an accessible name", async ({ page }) => {
  await openList(page, "/m/inbox");
  await pressSequence(page, "Enter");
  await expect(reader(page)).toBeVisible();
  await expect(mailRows(page).first()).toBeVisible();
  const unnamed = await page.getByRole("button").evaluateAll((buttons) =>
    buttons
      .filter((button) => {
        const box = button.getBoundingClientRect();
        if (box.width === 0 || box.height === 0) return false;
        if (button.closest("[aria-hidden=true]")) return false;
        const name =
          button.getAttribute("aria-label")?.trim() ||
          button.getAttribute("title")?.trim() ||
          button.textContent?.trim();
        return !name;
      })
      .map((button) => button.outerHTML.slice(0, 160)),
  );
  expect(unnamed).toEqual([]);
});
