import { expect, test, type Locator, type Page } from "@playwright/test";

import { openApp, restartDaemon, stopDaemon } from "./helpers/state";

/*
 * The map (docs/blueprint/26, D133): the rail's groups and its collapsed
 * form, the phone's More sheet and its active tab, the one settings nav, and
 * a rail that doesn't change shape when the daemon goes away.
 */

const sidebar = (page: Page) => page.getByRole("complementary", { name: "Mailboxes" });

/** The rail's labels in order, without counts or key hints. */
async function railLabels(page: Page): Promise<string[]> {
  return sidebar(page)
    .locator("[data-nav-index]")
    .evaluateAll((links) =>
      links.map((link) => link.querySelector("span.truncate")?.textContent?.trim() ?? ""),
    );
}

async function openGroups(page: Page): Promise<void> {
  for (const group of ["Mail", "Views", "Tools", "Labels"]) {
    const toggle = sidebar(page).getByRole("button", {
      name: group,
      exact: true,
    });
    if ((await toggle.count()) > 0 && (await toggle.getAttribute("aria-expanded")) === "false") {
      await toggle.click();
    }
  }
}

test.describe("the rail at 1440px", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("leads with the modes and Inbox, then Mail, Views, Tools and Labels, folded", async ({
    page,
  }) => {
    await openApp(page, "/now");
    await expect(sidebar(page).getByRole("link", { name: /^Inbox/ })).toBeVisible();
    // Folded until opened: the modes and Inbox are all a first look shows.
    expect(await railLabels(page)).toEqual([
      "Now",
      "Messages",
      "To do",
      "Updates",
      "Reading",
      "Archive",
      "Inbox",
    ]);
    await openGroups(page);
    const groups = sidebar(page).getByRole("navigation");
    expect(
      await groups.evaluateAll((navs) => navs.map((nav) => nav.getAttribute("aria-label"))),
    ).toEqual(expect.arrayContaining(["Places", "Mail", "Views", "Tools"]));
    const labels = await railLabels(page);
    expect(labels.slice(7, 15)).toEqual([
      "Starred",
      "Sent",
      "Drafts",
      "Snoozed",
      "Reply later",
      "All Mail",
      "Spam",
      "Trash",
    ]);
    expect(labels.slice(15, 21)).toEqual([
      "Waiting on",
      "Owed replies",
      "Screener",
      "Subscriptions",
      "Invites",
      "Deliveries",
    ]);
    expect(labels.slice(21, 29)).toEqual([
      "Search",
      "Analytics",
      "Rules",
      "Accounts",
      "Activity",
      "Jobs",
      "Diagnostics",
      "Settings",
    ]);
    // Labels come last, each with a 6px square of its colour before the name.
    const label = sidebar(page).getByRole("navigation", { name: "Labels" }).getByRole("link");
    await expect(label.first()).toBeVisible();
    const square = label.first().locator("span[style]");
    await expect(square).toHaveCSS("width", "6px");
    await expect(square).toHaveCSS("background-color", /^rgb\(/);
  });

  test("collapsed, the rail is the modes and Inbox with their keys in tooltips", async ({
    page,
  }) => {
    await openApp(page, "/now");
    await openGroups(page);
    await page.getByRole("button", { name: "Collapse sidebar" }).click();
    await expect(page.getByRole("button", { name: "Expand sidebar" })).toBeVisible();
    await expect(sidebar(page).getByRole("link")).toHaveCount(7);
    await expect(sidebar(page).getByRole("button", { name: "Mail", exact: true })).toHaveCount(0);
    await sidebar(page)
      .getByRole("link", { name: /^Messages/ })
      .hover();
    await expect(page.getByRole("tooltip")).toContainText("Messages");
    await expect(page.getByRole("tooltip")).toContainText("g m");
    // What the narrow rail leaves out is one control away, on every width.
    await sidebar(page).getByRole("button", { name: "More places" }).click();
    await expect(
      page.getByTestId("more-sheet").getByRole("link", { name: "Settings" }),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Expand sidebar" }).click();
  });

  test("settings is one page with its sections as tabs, and no second nav", async ({ page }) => {
    await openApp(page, "/settings/all");
    await expect(page).toHaveURL(/\/settings\/theme$/);
    const tabs = page.getByRole("navigation", { name: "Settings sections" });
    await expect(tabs).toHaveCount(1);
    await expect(tabs.getByRole("link", { name: "Appearance" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await tabs.getByRole("link", { name: "Keyboard" }).click();
    await expect(page).toHaveURL(/\/settings\/keybindings$/);
    await expect(page.getByRole("heading", { level: 1, name: "Keyboard" })).toBeVisible();
    // The selected tab follows the URL, also when it changes without a click.
    await page.goBack();
    await expect(page).toHaveURL(/\/settings\/theme$/);
    await expect(tabs.getByRole("link", { name: "Appearance" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(tabs.getByRole("link", { name: "Keyboard" })).not.toHaveAttribute(
      "aria-current",
      "page",
    );
    await page.goForward();
    await expect(tabs.getByRole("link", { name: "Keyboard" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  test("the rail keeps the same items when the daemon stops", async ({ page }) => {
    test.setTimeout(90_000);
    await openApp(page, "/m/inbox");
    await expect(sidebar(page).getByRole("link", { name: /^Inbox/ })).toBeVisible();
    await openGroups(page);
    const before = await railLabels(page);
    expect(before.length).toBeGreaterThan(28);
    await stopDaemon();
    let restarted = false;
    try {
      await expect(page.locator('[data-offline-banner="daemon"]')).toBeVisible({
        timeout: 15_000,
      });
      // A moment for the shell's retries to fail: the items must not move.
      await page.waitForTimeout(3_000);
      expect(await railLabels(page)).toEqual(before);
      await restartDaemon();
      restarted = true;
    } finally {
      if (!restarted) await restartDaemon();
    }
    await expect(page.locator('[data-offline-banner="daemon"]')).toBeHidden({
      timeout: 30_000,
    });
    expect(await railLabels(page)).toEqual(before);
  });
});

test.describe("the phone at 390px", () => {
  test.use({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    isMobile: true,
  });

  const tabs = (page: Page) => page.getByTestId("mobile-tabs");

  /** The one tab that is lit, by name. */
  async function litTab(page: Page): Promise<string[]> {
    return tabs(page)
      .locator('[aria-current="page"]')
      .evaluateAll((nodes) =>
        // Now's badge is part of its name ("Now, 12").
        nodes.map((node) => (node.textContent?.trim() ?? "").replace(/, \d+$/, "")),
      );
  }

  /** The lit tab settles after the route does, so it is polled. */
  async function expectLit(page: Page, names: string[]): Promise<void> {
    await expect.poll(() => litTab(page)).toEqual(names);
  }

  async function openMore(page: Page): Promise<Locator> {
    await tabs(page).getByRole("button", { name: "More" }).click();
    const sheet = page.getByTestId("more-sheet");
    await expect(sheet).toBeVisible();
    return sheet;
  }

  const DESTINATIONS: [group: string | null, name: string, url: RegExp][] = [
    [null, "Updates", /\/updates$/],
    [null, "Archive", /\/archive$/],
    [null, "Inbox", /\/m\/inbox$/],
    ["Mail", "Starred", /\/m\/starred$/],
    ["Mail", "Sent", /\/m\/sent$/],
    ["Mail", "Drafts", /\/drafts$/],
    ["Mail", "Snoozed", /\/snoozed$/],
    ["Mail", "Reply later", /\/reply-queue$/],
    ["Mail", "All Mail", /\/m\/archive$/],
    ["Mail", "Spam", /\/m\/spam$/],
    ["Mail", "Trash", /\/m\/trash$/],
    ["Views", "Waiting on", /\/messages\?turn=theirs$/],
    ["Views", "Owed replies", /\/owed$/],
    ["Views", "Screener", /\/screener(\?.*)?$/],
    ["Views", "Subscriptions", /\/subscriptions$/],
    ["Views", "Invites", /\/invites$/],
    ["Views", "Deliveries", /\/deliveries$/],
    ["Tools", "Search", /\/search$/],
    ["Tools", "Analytics", /\/analytics(\/[a-z-]+)?$/],
    ["Tools", "Rules", /\/rules$/],
    ["Tools", "Accounts", /\/accounts$/],
    ["Tools", "Activity", /\/activity$/],
    ["Tools", "Jobs", /\/jobs$/],
    ["Tools", "Diagnostics", /\/diagnostics$/],
    ["Tools", "Settings", /\/settings\/all$/],
  ];

  test("every destination is reachable from More, and More is the lit tab on each", async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await openApp(page, "/now");
    await expect(tabs(page)).toBeVisible();
    await expectLit(page, ["Now"]);
    for (const [group, name, url] of DESTINATIONS) {
      await test.step(name, async () => {
        const sheet = await openMore(page);
        const scope = group ? sheet.getByRole("navigation", { name: group }) : sheet;
        // A count follows the name ("Screener 28").
        await scope.getByRole("link", { name: new RegExp(`^${name}(\\s*\\d+)?$`) }).click();
        await expect(page).toHaveURL(url);
        await expect(page.getByTestId("more-sheet")).toHaveCount(0);
        await expectLit(page, ["More"]);
      });
    }
    // A label, from the Labels group.
    const sheet = await openMore(page);
    const label = sheet.getByRole("navigation", { name: "Labels" }).getByRole("link").first();
    await expect(label).toBeVisible();
    await label.click();
    await expect(page).toHaveURL(/\/m\/label\//);
    await expectLit(page, ["More"]);
  });

  test("the four mode tabs light themselves and nothing else", async ({ page }) => {
    await openApp(page, "/now");
    for (const [name, url] of [
      ["Messages", /\/messages$/],
      ["To do", /\/todo$/],
      ["Reading", /\/reading$/],
      ["Now", /\/now$/],
    ] as const) {
      await tabs(page).getByRole("link", { name }).click();
      await expect(page).toHaveURL(url);
      await expectLit(page, [name]);
    }
    // Updates is in the sheet: More lights, not Find or Now.
    await openApp(page, "/updates");
    await expectLit(page, ["More"]);
  });

  test("the sheet traps focus, closes on Esc and on navigation, and holds the page's keys", async ({
    page,
  }) => {
    await openApp(page, "/now");
    const sheet = await openMore(page);
    for (let press = 0; press < 40; press += 1) {
      await page.keyboard.press("Tab");
      expect(await sheet.evaluate((node) => node.contains(document.activeElement))).toBe(true);
    }
    // The page under the sheet gets no keys while it is open.
    await page.keyboard.press("g");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/now$/);
    await page.keyboard.press("Escape");
    await expect(sheet).toHaveCount(0);
    await expect(tabs(page).getByRole("button", { name: "More" })).toBeFocused();
    // Closed, the same keys reach the page again.
    await page.keyboard.press("g");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/messages$/);
    // A navigation from outside the sheet closes it too.
    const again = await openMore(page);
    await page.evaluate(() => history.back());
    await expect(again).toHaveCount(0);
  });

  test("Settings drills in from a list, with no clipped tab strip", async ({ page }) => {
    await openApp(page, "/settings/all");
    const list = page.getByRole("list", { name: "Settings sections" });
    await expect(list).toBeVisible();
    await list.getByRole("link", { name: /^Keyboard/ }).click();
    await expect(page).toHaveURL(/\/settings\/keybindings$/);
    await expect(page.getByRole("navigation", { name: "Settings sections" })).toHaveCount(0);
    await page.getByRole("link", { name: "Settings", exact: true }).first().click();
    await expect(page).toHaveURL(/\/settings\/all$/);
    await expectLit(page, ["More"]);
  });

  test("the top bar's search is the only search field on the phone", async ({ page }) => {
    await openApp(page, "/find");
    await expect(page.getByRole("heading", { level: 1, name: "Find" })).toBeVisible();
    await expect(page.getByRole("searchbox")).toHaveCount(0);
    await expect(page.getByRole("textbox")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Search mail" })).toBeVisible();
  });
});
