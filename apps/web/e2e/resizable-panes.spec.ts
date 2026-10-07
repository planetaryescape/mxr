import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Locator, type Page } from "@playwright/test";

import { mailList, modKey, openList, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

/*
 * Every list/detail split and the sidebar resize from a handle
 * (components/ui/resizable.tsx): drag, arrow keys, double-click to reset,
 * min and max clamps, and a saved width that is there on the first frame
 * after a reload. Phones have no handles.
 */

test.use({ viewport: { width: 1440, height: 900 } });

/** Widths sampled on each frame of a load, by the init script below. */
interface FrameWidths {
  people: number[];
  sidebar: number[];
}

declare global {
  interface Window {
    __widths?: FrameWidths;
  }
}

const PEOPLE = '[id="messages-people"]';
const SIDEBAR = '[id="shell-sidebar"]';
const REM = 16;

function handle(page: Page, name: string | RegExp): Locator {
  return page.getByRole("separator", { name });
}

async function width(page: Page, selector: string): Promise<number> {
  return page.locator(selector).evaluate((element) => element.getBoundingClientRect().width);
}

async function drag(page: Page, target: Locator, dx: number): Promise<void> {
  const box = await target.boundingBox();
  expect(box, "handle has a box").not.toBeNull();
  const x = box!.x + box!.width / 2;
  const y = box!.y + box!.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx, y, { steps: 12 });
  await page.mouse.up();
}

async function openMessages(page: Page) {
  await openApp(page, "/messages");
  await expect(handle(page, "Resize people list")).toBeVisible();
}

/** The thread id of the inbox's first conversation, opened in the reader. */
async function openFirstThread(page: Page): Promise<string> {
  await openList(page, "/m/inbox");
  await page.keyboard.press("Enter");
  await expect(reader(page)).toBeVisible();
  const thread = /\/m\/inbox\/([^/?#]+)/.exec(page.url())?.[1];
  if (!thread) throw new Error(`the reader is not at /m/inbox/<thread>: ${page.url()}`);
  return thread;
}

test("dragging the people list's handle resizes it, a reload keeps it and double-click resets it", async ({
  page,
}) => {
  await openMessages(page);
  const people = handle(page, "Resize people list");
  expect(await width(page, PEOPLE)).toBeCloseTo(22 * REM, 0);

  await drag(page, people, 120);
  const dragged = await width(page, PEOPLE);
  expect(dragged).toBeGreaterThan(22 * REM + 100);

  await page.reload();
  await expect(people).toBeVisible();
  await expect.poll(() => width(page, PEOPLE)).toBeCloseTo(dragged, 0);

  await people.dblclick();
  await expect.poll(() => width(page, PEOPLE)).toBeCloseTo(22 * REM, 0);
});

test("the arrow keys resize from the focused handle, which shows a focus ring", async ({
  page,
}) => {
  await openMessages(page);
  const people = handle(page, "Resize people list");
  await people.focus();
  const before = await width(page, PEOPLE);
  await page.keyboard.press("ArrowRight");
  await expect.poll(() => width(page, PEOPLE)).toBeGreaterThan(before + 10);
  const wider = await width(page, PEOPLE);
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await expect.poll(() => width(page, PEOPLE)).toBeLessThan(wider - 10);
  await expect(people).toBeFocused();
  const ring = await people.evaluate((element) => getComputedStyle(element).boxShadow);
  expect(ring).not.toBe("none");
});

test("the people list stops at 16rem and 40rem", async ({ page }) => {
  await openMessages(page);
  const people = handle(page, "Resize people list");
  await drag(page, people, -600);
  await expect.poll(() => width(page, PEOPLE)).toBeCloseTo(16 * REM, 0);
  await drag(page, people, 900);
  await expect.poll(() => width(page, PEOPLE)).toBeCloseTo(40 * REM, 0);
});

test("the sidebar resizes, keeps its width, stops at its max and collapses below its min", async ({
  page,
}) => {
  await openMessages(page);
  const sidebar = handle(page, "Resize sidebar");
  expect(await width(page, SIDEBAR)).toBeCloseTo(248, 0);

  await drag(page, sidebar, 60);
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(308, 0);
  await page.reload();
  await expect(sidebar).toBeVisible();
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(308, 0);

  await drag(page, sidebar, 600);
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(400, 0);

  // Past the minimum it snaps to the icon rail, and the button follows.
  await drag(page, sidebar, -330);
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(56, 0);
  const expand = page.getByRole("button", { name: "Expand sidebar" });
  await expect(expand).toBeVisible();

  // The button brings back the width it had.
  await expand.click();
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(400, 0);
  await page.getByRole("button", { name: "Collapse sidebar" }).click();
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(56, 0);
  await page.reload();
  await expect(page.getByRole("button", { name: "Expand sidebar" })).toBeVisible();
  expect(await width(page, SIDEBAR)).toBeCloseTo(56, 0);
});

test("the sidebar's saved width comes back after a narrow window pins it to the rail", async ({
  page,
}) => {
  await openMessages(page);
  await drag(page, handle(page, "Resize sidebar"), 60);
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(308, 0);
  await page.setViewportSize({ width: 768, height: 900 });
  await page.reload();
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(56, 0);
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(308, 0);
  await page.reload();
  await expect.poll(() => width(page, SIDEBAR)).toBeCloseTo(308, 0);
});

test("a saved width is there on the first frame, with no shift as the page loads", async ({
  page,
}) => {
  await openMessages(page);
  await drag(page, handle(page, "Resize people list"), 100);
  await drag(page, handle(page, "Resize sidebar"), 40);
  const people = await width(page, PEOPLE);
  const sidebar = await width(page, SIDEBAR);

  // Sample both widths on every frame from the first one that has them.
  await page.addInitScript(
    ({ peopleSelector, sidebarSelector }) => {
      const seen: FrameWidths = { people: [], sidebar: [] };
      window.__widths = seen;
      const sample = () => {
        const list = document.querySelector(peopleSelector);
        const rail = document.querySelector(sidebarSelector);
        if (list) seen.people.push(Math.round(list.getBoundingClientRect().width));
        if (rail) seen.sidebar.push(Math.round(rail.getBoundingClientRect().width));
        if (seen.people.length < 120) requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    },
    { peopleSelector: PEOPLE, sidebarSelector: SIDEBAR },
  );
  await page.reload();
  await expect(handle(page, "Resize people list")).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => window.__widths?.people.length ?? 0))
    .toBeGreaterThanOrEqual(120);
  const widths = await page.evaluate(() => window.__widths ?? { people: [], sidebar: [] });
  expect(new Set(widths.people)).toEqual(new Set([Math.round(people)]));
  expect(new Set(widths.sidebar)).toEqual(new Set([Math.round(sidebar)]));
});

test("mail lists resize beside the reader and share one saved width", async ({ page }) => {
  const thread = await openFirstThread(page);
  const list = handle(page, "Resize Inbox list");
  await expect(list).toBeVisible();
  const before = await width(page, '[id="list-pane"]');
  await drag(page, list, 80);
  await expect.poll(() => width(page, '[id="list-pane"]')).toBeCloseTo(before + 80, 0);

  await openApp(page, `/search/${thread}?q=canary`);
  await expect(reader(page)).toBeVisible();
  await expect(handle(page, /^Resize .+ list$/)).toBeVisible();
  await expect.poll(() => width(page, '[id="list-pane"]')).toBeCloseTo(before + 80, 0);
});

test("To do and Updates resize their list beside an open conversation", async ({ page }) => {
  const thread = await openFirstThread(page);
  for (const [path, name] of [
    ["/todo", "Resize To do list"],
    ["/updates", /^Resize .+ list$/],
  ] as const) {
    await openApp(page, `${path}/${thread}`);
    const list = handle(page, name);
    await expect(list).toBeVisible();
    const before = await width(page, '[id="place-list"]');
    await drag(page, list, -60);
    await expect.poll(() => width(page, '[id="place-list"]')).toBeCloseTo(before - 60, 0);
  }
});

test("Archive's record card resizes beside the ledger", async ({ page }) => {
  await openApp(page, "/archive");
  const card = handle(page, "Resize record card");
  await expect(card).toBeVisible();
  const before = await width(page, '[id="archive-card"]');
  await drag(page, card, -80);
  await expect.poll(() => width(page, '[id="archive-card"]')).toBeCloseTo(before + 80, 0);
  await card.dblclick();
  await expect.poll(() => width(page, '[id="archive-card"]')).toBeCloseTo(before, 0);
});

/** The reader's floor: never squeezed below this by a saved list width. */
const READER_MIN = 28 * REM;

test("at 768px a saved wide people list gives way to the conversation, and comes back", async ({
  page,
}) => {
  await openMessages(page);
  await handle(page, "Resize people list").focus();
  await page.keyboard.press("End");
  await expect.poll(() => width(page, PEOPLE)).toBeCloseTo(40 * REM, 0);

  await page.setViewportSize({ width: 768, height: 900 });
  await page.reload();
  await expect(handle(page, "Resize people list")).toBeVisible();
  await expect
    .poll(() => width(page, '[id="messages-conversation"]'))
    .toBeGreaterThanOrEqual(READER_MIN - 1);

  // The saved width was a preference, not a casualty of the small window.
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect.poll(() => width(page, PEOPLE)).toBeCloseTo(40 * REM, 0);
  await page.reload();
  await expect.poll(() => width(page, PEOPLE)).toBeCloseTo(40 * REM, 0);
});

test("at 1024px with the context rail open, a saved wide list leaves the reader its minimum", async ({
  page,
}) => {
  await openFirstThread(page);
  // End takes it to its maximum (a long drag would cross the email's iframe).
  await handle(page, "Resize Inbox list").focus();
  await page.keyboard.press("End");
  await expect.poll(() => width(page, '[id="list-pane"]')).toBeCloseTo(40 * REM, 0);

  await page.setViewportSize({ width: 1024, height: 900 });
  await page.reload();
  await expect(reader(page)).toBeVisible();
  await page.keyboard.press(`${await modKey(page)}+k`);
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await expect(palette).toBeVisible();
  await palette.getByRole("combobox").fill("Find an expert");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("complementary", { name: "Context" })).toBeVisible();
  await expect.poll(() => width(page, '[id="reader-pane"]')).toBeGreaterThanOrEqual(READER_MIN - 1);

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.reload();
  await expect.poll(() => width(page, '[id="list-pane"]')).toBeCloseTo(40 * REM, 0);
});

test.describe("at 390px", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("a reply drafted on a phone survives going back to the people and reopening", async ({
    page,
  }) => {
    await openApp(page, "/messages");
    const row = page.locator("[data-row-id]").first();
    await row.click();
    await page
      .getByRole("button", { name: /^Reply to / })
      .first()
      .click();
    const composer = page.locator("#inline-composer-slot [data-compose-surface]");
    const body = composer.locator(".cm-content");
    await expect(body).toBeVisible();
    await body.click();
    await page.keyboard.press("i");
    await page.keyboard.type("drafted on the bus");
    await page.keyboard.press("Escape");

    await page.getByRole("button", { name: "Back to the people" }).click();
    await expect(row).toBeVisible();
    await row.click();

    await expect(body).toBeVisible();
    await expect(body).toContainText("drafted on the bus");
    await body.click();
    await page.keyboard.press("A");
    await page.keyboard.type(", still here");
    await page.keyboard.press("Escape");
    await expect(body).toContainText("drafted on the bus, still here");
  });

  test("no split shows a handle", async ({ page }) => {
    for (const path of ["/messages", "/m/inbox", "/todo", "/updates", "/archive"]) {
      await openApp(page, path);
      await expect(page.locator("#main")).toBeVisible();
      await page.waitForLoadState("networkidle");
      await expect(page.getByRole("separator"), path).toHaveCount(0);
    }
    await openList(page, "/m/inbox");
    await page.keyboard.press("Enter");
    await expect(reader(page)).toBeVisible();
    await expect(mailList(page)).toBeHidden();
    await expect(page.getByRole("separator")).toHaveCount(0);
  });
});

for (const colorScheme of ["dark", "light"] as const) {
  test.describe(`axe with handles, ${colorScheme}`, () => {
    test.use({ colorScheme });

    test("no serious or critical violations on Messages and an open conversation", async ({
      page,
    }) => {
      for (const open of [openMessages, openFirstThread]) {
        await open(page);
        await page.waitForLoadState("networkidle");
        await handle(page, /^Resize /)
          .last()
          .focus();
        const results = await new AxeBuilder({ page })
          .withTags(["wcag2a", "wcag2aa"])
          .exclude("iframe")
          .analyze();
        const blocking = results.violations
          .filter((violation) => violation.impact === "serious" || violation.impact === "critical")
          .flatMap((violation) => violation.nodes.map((node) => `${violation.id}: ${node.target}`));
        expect(blocking).toEqual([]);
      }
    });
  });
}
