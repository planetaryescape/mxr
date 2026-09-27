import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

import { mailRows, openList, pressSequence, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface KnownViolation {
  id: string;
  /** Matches the axe target selector of the offending node. */
  target: RegExp;
  bug: string;
}

/*
 * Reported app bugs. A node matching one of these is tolerated so the gate
 * still catches every new violation; each has a fixme test below that fails
 * until the bug is fixed, then its entry goes.
 */
const SIDEBAR_HEADER_CONTRAST: KnownViolation = {
  id: "color-contrast",
  target: /^nav\[aria-label="[^"]+"\] > /,
  bug: "sidebar section headers (Triage, Labels, Tools): #587088 on #06121e at 10.5px is 3.67:1",
};
const LIST_GROUP_HEADER_CONTRAST: KnownViolation = {
  id: "color-contrast",
  target: /^div\[data-index="\d+"\] > \.h-\\\[30px\\\]/,
  bug: "mail list date-group headers (Yesterday, Today): text-faint at 10.5px fails 4.5:1",
};
const READER_BADGE_CONTRAST: KnownViolation = {
  id: "color-contrast",
  target: /\[\\&_svg\\\]\\:size-3/,
  bug: "reader header badge (components/ui/badge.tsx) text fails 4.5:1",
};
const READER_HEADER_NESTED: KnownViolation = {
  id: "nested-interactive",
  target: /\.text-left\.flex-1\.min-w-0/,
  bug: "expanded message header toggle button contains the recipients button",
};
const ROW_NESTED_INTERACTIVE: KnownViolation = {
  id: "nested-interactive",
  target: /^#mail-row-/,
  bug: "mail rows (role=option) contain buttons: the avatar checkbox and hover quick actions",
};
const HELP_SCROLL_REGION: KnownViolation = {
  id: "scrollable-region-focusable",
  target: /overflow-auto/,
  bug: "keyboard help's scrolling shortcut list is not focusable",
};
const COMPOSE_FILE_INPUT: KnownViolation = {
  id: "label",
  target: /^input\[multiple/,
  bug: "compose's hidden attachment file input has no label",
};
const COMPOSE_BODY_NAME: KnownViolation = {
  id: "aria-input-field-name",
  target: /\.cm-content/,
  bug: "the CodeMirror body editor (role=textbox) intermittently has no accessible name",
};
const SHELL_KNOWN = [SIDEBAR_HEADER_CONTRAST, LIST_GROUP_HEADER_CONTRAST, ROW_NESTED_INTERACTIVE];

/** Serious and critical axe violations, one line per node, minus `known`. */
async function blockingViolations(page: Page, known: KnownViolation[] = SHELL_KNOWN): Promise<string[]> {
  // Message bodies render in sandboxed srcdoc iframes that forbid scripts, so
  // axe cannot run inside them; they hold untrusted email, not app UI.
  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa"])
    .exclude("iframe")
    .analyze();
  return results.violations
    .filter((violation) => violation.impact === "serious" || violation.impact === "critical")
    .flatMap((violation) =>
      violation.nodes
        .map((node) => node.target.join(" "))
        .filter(
          (target) =>
            !known.some((entry) => entry.id === violation.id && entry.target.test(target)),
        )
        .map((target) => `${violation.id} (${violation.impact}): ${target}`),
    );
}

async function hasViolation(page: Page, known: KnownViolation): Promise<boolean> {
  const remaining = await blockingViolations(page, []);
  return remaining.some((line) => {
    const [id, target] = [line.slice(0, line.indexOf(" (")), line.slice(line.indexOf("): ") + 3)];
    return id === known.id && known.target.test(target);
  });
}

const ROUTES = [
  "/m/inbox",
  "/m/label/work",
  "/search?q=canary",
  "/snoozed",
  "/drafts",
  "/rules",
  "/settings/theme",
  "/onboarding",
];

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
  expect(
    await blockingViolations(page, [...SHELL_KNOWN, READER_BADGE_CONTRAST, READER_HEADER_NESTED]),
  ).toEqual([]);
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
  expect(await blockingViolations(page, [...SHELL_KNOWN, HELP_SCROLL_REGION])).toEqual([]);
});

test("axe: compose overlay", async ({ page }) => {
  await openList(page, "/m/inbox");
  await page.keyboard.press("c");
  const composer = page.getByRole("dialog", { name: "New message" });
  // Let the lazy CodeMirror editor mount and label itself before scanning.
  await expect(composer.getByRole("group", { name: "Message body" }).locator(".cm-content")).toBeVisible();
  expect(
    await blockingViolations(page, [...SHELL_KNOWN, COMPOSE_FILE_INPUT, COMPOSE_BODY_NAME]),
  ).toEqual([]);
});

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

const KNOWN_BUGS: [KnownViolation, (page: Page) => Promise<void>][] = [
  [SIDEBAR_HEADER_CONTRAST, (page) => openList(page, "/m/inbox")],
  [LIST_GROUP_HEADER_CONTRAST, (page) => openList(page, "/m/inbox")],
  [ROW_NESTED_INTERACTIVE, (page) => openList(page, "/m/inbox")],
  ...[READER_BADGE_CONTRAST, READER_HEADER_NESTED].map(
    (known): [KnownViolation, (page: Page) => Promise<void>] => [
      known,
      async (page) => {
        await openList(page, "/m/inbox");
        await page.keyboard.press("Enter");
        await expect(reader(page)).toBeVisible();
      },
    ],
  ),
  [
    HELP_SCROLL_REGION,
    async (page) => {
      await openList(page, "/m/inbox");
      await page.keyboard.press("?");
      await expect(page.getByRole("dialog", { name: "Keyboard" })).toBeVisible();
    },
  ],
  ...[COMPOSE_FILE_INPUT, COMPOSE_BODY_NAME].map(
    (known): [KnownViolation, (page: Page) => Promise<void>] => [
      known,
      async (page) => {
        await openList(page, "/m/inbox");
        await page.keyboard.press("c");
        await expect(page.getByRole("dialog", { name: "New message" })).toBeVisible();
      },
    ],
  ),
];

for (const [known, arrive] of KNOWN_BUGS) {
  // BUG: see `known.bug`; remove the entry above once this passes.
  test.fixme(`axe bug: ${known.bug}`, async ({ page }) => {
    await arrive(page);
    expect(await hasViolation(page, known)).toBe(false);
  });
}
