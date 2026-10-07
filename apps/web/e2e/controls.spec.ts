import { expect, test, type Locator, type Page } from "@playwright/test";

import { mailList, openList, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

/**
 * The persistent controls on each surface, by accessible name. Each one is
 * justified in docs/web-app-controls.md; adding or removing a control means
 * updating that inventory and this list together.
 */
const INVENTORY = {
  topbar: ["Search mail", "Compose new email"],
  // "Latest mail 5m ago": the age changes, the control does not.
  statusBar: ["Sync now", "Latest mail", "all keys"],
  listHeader: ["Show single messages"],
  readerHeader: [
    "Close (Esc)",
    "Previous conversation (N)",
    "Next conversation (n)",
    "Archive (e)",
    "Snooze (Z)",
    "More actions",
  ],
  readerViews: ["Formatted", "Reader", "Plain"],
  messageCard: ["Star message", "Reply to this message", "More for this message"],
  // The sender and "to …" lines are the message itself (they fold it), not chrome.,
  focusHeader: ["Leave"],
  focusQueue: [
    "Send and next",
    "Skip",
    "Done, no reply needed",
    "Snooze",
    "Send, remind if no reply",
    "Draft in your voice",
  ],
  readingHeader: ["Let go of all"],
  updatesHeader: [],
} as const;

async function names(locator: Locator, role: "button" | "radio" = "button"): Promise<string[]> {
  const controls = locator.getByRole(role);
  // The name a person reads: the aria-label, or the words without key chips.
  const all = await controls.evaluateAll((nodes) =>
    nodes
      .filter((node) => (node as HTMLElement).offsetParent !== null)
      .map((node) => {
        const label = node.getAttribute("aria-label");
        if (label) return label;
        const copy = node.cloneNode(true) as HTMLElement;
        for (const chip of copy.querySelectorAll("kbd")) chip.remove();
        return copy.textContent?.replace(/\s+/g, " ").trim() ?? "";
      }),
  );
  return all;
}

async function openFirstThread(page: Page) {
  await openList(page, "/m/inbox");
  await mailList(page)
    .getByRole("option", { name: /Re: Security review follow-up/ })
    .first()
    .click();
  await expect(reader(page)).toBeVisible();
}

test("the shell, the list and the reader keep exactly their inventoried controls", async ({
  page,
}) => {
  await openFirstThread(page);
  expect(await names(page.getByRole("banner").first())).toEqual(INVENTORY.topbar);
  expect(
    (await names(page.getByRole("contentinfo"))).map((name) =>
      name.startsWith("Latest mail") ? "Latest mail" : name,
    ),
  ).toEqual(INVENTORY.statusBar);
  const list = page.getByRole("region", { name: "Inbox" });
  expect(await names(list.locator("header").first())).toEqual(INVENTORY.listHeader);
  const header = reader(page).locator("header").first();
  expect(await names(header)).toEqual(INVENTORY.readerHeader);
  expect(await names(header, "radio")).toEqual(INVENTORY.readerViews);
  const newest = reader(page).getByTestId("thread-message").last();
  const cardControls = (await names(newest.locator("header").first()))
    .map((name) => name.replace(/^Unstar/, "Star"))
    .filter(
      (name) => INVENTORY.messageCard.some((known) => known === name) || /message$/.test(name),
    );
  expect(cardControls).toEqual(INVENTORY.messageCard);
});

test("the desk heading holds links, never buttons", async ({ page }) => {
  await openApp(page, "/desk");
  const heading = page.getByRole("heading", { level: 1 });
  await expect(heading).toBeVisible();
  expect(await names(page.getByRole("region", { name: "Desk" }).locator("header").first())).toEqual(
    [],
  );
});

test("every desk row ends in one Done check, and nothing else", async ({ page }) => {
  await openList(page, "/desk");
  const rows = mailList(page).getByRole("option");
  const count = await rows.count();
  expect(count).toBeGreaterThan(0);
  // The check is a pointer target beside the key (`e`): rows are listbox
  // options, so it isn't a button to assistive tech.
  await expect(mailList(page).getByTestId("desk-done")).toHaveCount(count);
  for (const row of await rows.all()) {
    await expect(row.getByTestId("desk-done")).toHaveCount(1);
  }
});

test("focus mode and the places keep exactly their inventoried controls", async ({ page }) => {
  await openApp(page, "/focus");
  await expect(page.getByTestId("focus-progress")).toBeVisible();
  const focus = page.getByLabel("Focus and reply");
  expect(await names(focus.locator("header").first())).toEqual(INVENTORY.focusHeader);
  expect(await names(page.getByRole("toolbar", { name: "Move through the queue" }))).toEqual(
    INVENTORY.focusQueue,
  );

  await openApp(page, "/reading");
  await expect(page.getByRole("heading", { level: 1, name: "Reading", exact: true })).toBeVisible();
  expect(
    await names(page.getByRole("region", { name: "Reading" }).locator("header").first()),
  ).toEqual(INVENTORY.readingHeader);
  await openApp(page, "/updates");
  await expect(page.getByRole("heading", { level: 1, name: "Updates", exact: true })).toBeVisible();
  expect(
    await names(page.getByRole("region", { name: "Updates" }).locator("header").first()),
  ).toEqual(INVENTORY.updatesHeader);
});
