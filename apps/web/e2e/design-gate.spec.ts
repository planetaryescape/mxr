import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Page } from "@playwright/test";

import { mockAssistant } from "./helpers/assistant";
import { mailRows, reader } from "./helpers/mail";
import { openApp } from "./helpers/state";

/*
 * The design gate (docs/blueprint/26, D138): every main page in every theme at
 * phone, laptop and desktop widths, as screenshots attached to the run for
 * review against the concept, plus the one-frame rules a screenshot can't
 * prove: one page title per frame, the title the largest text, and the
 * frame's height fixed per width.
 *
 * MXR_DESIGN_GATE_DIR also writes each screenshot to that folder.
 */

const THEMES = ["midnight", "light", "paper", "eclipse"] as const;
const WIDTHS = [390, 1024, 1440] as const;

interface GatePage {
  name: string;
  path: string;
  /** The frame always has tabs, so it is taller. */
  tabs?: boolean;
  /** A detail pane opens beside the list: one Display title, and the two headers' rules line up. */
  detail?: boolean;
  /** Opens the first conversation: the reader frame wraps long titles, so its height is a minimum. */
  thread?: boolean;
}

const PAGES: GatePage[] = [
  { name: "now", path: "/now" },
  { name: "messages", path: "/messages" },
  { name: "todo", path: "/todo" },
  { name: "updates", path: "/updates" },
  { name: "reading", path: "/reading", tabs: true },
  { name: "archive", path: "/archive" },
  { name: "inbox", path: "/m/inbox" },
  { name: "thread", path: "/m/inbox", thread: true, detail: true },
  {
    name: "person",
    path: "/messages?person=person%3Asamir%40launchpad.example",
    detail: true,
  },
  { name: "settings", path: "/settings/theme" },
  { name: "screener", path: "/screener", tabs: true },
];

/** Heights from docs/blueprint/27 "The page frame": 88 or 120 from 640px, shorter on phones. */
function frameHeight(width: number, tabs: boolean): number {
  if (width >= 640) return tabs ? 120 : 88;
  return tabs ? 96 : 64;
}

/** A phone shows the list or the reader, not both, so the hidden pane's frame is not the page's. */
function visibleFrames(page: Page) {
  return page.locator("#main").getByTestId("page-frame").filter({ visible: true });
}

async function openPage(page: Page, gate: GatePage): Promise<void> {
  await openApp(page, gate.path);
  await expect(page.locator("#main")).toBeVisible();
  if (gate.thread) {
    await expect(mailRows(page).first()).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(reader(page)).toBeVisible();
  }
  await expect(visibleFrames(page).first()).toBeVisible();
  // Let rows and fonts settle before the picture.
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(400);
}

/** The largest computed font size among visible text in `#main`, excluding `skip`. */
async function largestOtherText(
  page: Page,
): Promise<{ title: number; other: number; text: string }> {
  return page.evaluate(() => {
    const main = document.querySelector("#main");
    // A phone parks the list pane while a conversation is open: its h1 is not on screen.
    const title = [...(main?.querySelectorAll("h1") ?? [])].find(
      (heading) => heading.getBoundingClientRect().width > 0,
    );
    if (!main || !title) throw new Error("no page title");
    const titleSize = parseFloat(getComputedStyle(title).fontSize);
    let other = 0;
    let text = "";
    const walker = document.createTreeWalker(main, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const parent = node.parentElement;
      // Beside a reader, the list has its own frame and title: one per pane.
      if (!parent || !node.textContent?.trim() || parent.closest("h1")) continue;
      // Mail bodies are the sender's pixels, not the app's type.
      if (parent.closest("iframe, [data-testid='thread-message'], .reading-themed")) continue;
      const rect = parent.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      const size = parseFloat(getComputedStyle(parent).fontSize);
      if (size > other) {
        other = size;
        text = node.textContent.trim().slice(0, 40);
      }
    }
    return { title: titleSize, other, text };
  });
}

for (const theme of THEMES) {
  test.describe(`${theme} theme`, () => {
    test.beforeEach(async ({ page }) => {
      await page.addInitScript((chosen) => {
        localStorage.setItem(
          "mxr.uiPrefs",
          JSON.stringify({ state: { theme: chosen }, version: 6 }),
        );
      }, theme);
    });

    for (const gate of PAGES) {
      test(`${gate.name} wears one page frame at every width`, async ({ page }, testInfo) => {
        for (const width of WIDTHS) {
          await test.step(`${width}px`, async () => {
            await page.setViewportSize({
              width,
              height: width === 390 ? 844 : 900,
            });
            await openPage(page, gate);
            await expect(page.locator("html")).toHaveAttribute("data-theme", theme);

            // One title per frame, and every visible frame has exactly one.
            const frames = visibleFrames(page);
            const titles = page.locator("#main h1:visible");
            const count = await frames.count();
            expect(count).toBeGreaterThan(0);
            await expect(titles).toHaveCount(count);
            for (let index = 0; index < count; index += 1) {
              await expect(frames.nth(index).locator("h1")).toHaveCount(1);
            }
            const size = width >= 1024 ? "type-display" : "type-title";
            await expect(titles.first()).toHaveClass(new RegExp(`\\b(type-title|${size})\\b`));

            // The title is the largest text on the page.
            const { title, other, text } = await largestOtherText(page);
            expect(title, `title ${title}px vs "${text}" ${other}px`).toBeGreaterThan(other);

            // Beside a detail pane the list steps down to Title, so one Display shows,
            // and both headers end on the same line.
            if (gate.detail && width >= 1024) {
              const bottoms = await frames.evaluateAll((nodes) =>
                nodes.map((node) => node.getBoundingClientRect().bottom),
              );
              expect(bottoms).toHaveLength(2);
              expect(Math.abs((bottoms[0] ?? 0) - (bottoms[1] ?? 0))).toBeLessThanOrEqual(1);
              const sizes = await titles.evaluateAll((nodes) =>
                nodes.map((node) => getComputedStyle(node).fontSize),
              );
              expect(sizes.filter((size) => size === "28px")).toHaveLength(1);
            }

            // The first frame is the page's own: its height is fixed per width.
            const box = await frames.first().boundingBox();
            // Settings' sections are the page's tabs from md up; below it they are a list.
            const tabs = gate.tabs ?? (gate.name === "settings" && width >= 768);
            await expect(frames.first()).toHaveAttribute("data-tabs", String(tabs));
            const expected = frameHeight(width, tabs);
            if (gate.thread) {
              expect(box?.height).toBeGreaterThanOrEqual(frameHeight(width, false));
            } else {
              // The rule under the frame is its last pixel.
              expect(Math.round(box?.height ?? 0)).toBe(expected + 1);
            }

            const body = await page.screenshot();
            const label = `${gate.name}-${theme}-${width}`;
            await testInfo.attach(label, { body, contentType: "image/png" });
            const dir = process.env.MXR_DESIGN_GATE_DIR;
            if (dir) {
              mkdirSync(dir, { recursive: true });
              writeFileSync(join(dir, `${label}.png`), body);
            }
          });
        }
      });
    }
  });
}

/** Rows on every list page: each draws one `Row` per item, with its own `data-lit`. */
const ROW_PAGES = [
  { name: "now", path: "/now", rows: '[data-testid="now-row"]' },
  {
    name: "messages",
    path: "/messages",
    rows: 'li:has([data-testid="messages-row"])',
  },
  { name: "todo", path: "/todo", rows: '[data-testid="todo-row"]' },
  { name: "updates", path: "/updates", rows: '[data-testid="update-line"]' },
  { name: "reading", path: "/reading", rows: '[data-testid="reading-item"]' },
  { name: "archive", path: "/archive", rows: '[data-testid="record-row"]' },
  {
    name: "inbox",
    path: "/m/inbox",
    rows: '[data-testid="mailbox-list"] [data-lit]',
  },
] as const;

/** The list pages whose rows carry the page's keys in the status strip. */
const KEY_PAGES = ROW_PAGES.filter((gate) => gate.name !== "now" && gate.name !== "messages");

async function rowBoxes(page: Page, selector: string) {
  return page.locator(selector).evaluateAll((nodes) =>
    nodes.map((node) => {
      const box = node.getBoundingClientRect();
      return { height: box.height, width: box.width };
    }),
  );
}

async function attach(
  page: Page,
  testInfo: {
    attach: (name: string, options: { body: Buffer; contentType: string }) => Promise<void>;
  },
  label: string,
) {
  const body = await page.screenshot();
  await testInfo.attach(label, { body, contentType: "image/png" });
  const dir = process.env.MXR_DESIGN_GATE_DIR;
  if (dir) {
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, `${label}.png`), body);
  }
}

for (const theme of THEMES) {
  test.describe(`${theme} theme: the lamp`, () => {
    test.beforeEach(async ({ page }) => {
      await page.addInitScript((chosen) => {
        localStorage.setItem(
          "mxr.uiPrefs",
          JSON.stringify({ state: { theme: chosen }, version: 6 }),
        );
      }, theme);
    });

    for (const gate of ROW_PAGES) {
      test(`${gate.name}: j lights exactly one row, and lighting changes no row's height`, async ({
        page,
      }, testInfo) => {
        for (const width of WIDTHS) {
          await test.step(`${width}px`, async () => {
            await page.setViewportSize({
              width,
              height: width === 390 ? 844 : 900,
            });
            await openApp(page, gate.path);
            await expect(page.locator(gate.rows).first()).toBeVisible();
            await page.evaluate(() => document.fonts.ready);
            const before = await rowBoxes(page, gate.rows);
            await page.keyboard.press("j");
            await expect(page.locator("#main [data-lit='true']")).toHaveCount(1);
            await page.waitForTimeout(250);
            const after = await rowBoxes(page, gate.rows);
            // Same rows, same size: the lit row keeps its height and nothing below it moves.
            // A phone's Messages swaps the list for the person's page on the first key, which
            // hides the list, so there is nothing to compare there.
            const listShown = gate.name !== "messages" || width >= 768;
            if (listShown) expect(after).toEqual(before);
            await attach(page, testInfo, `${gate.name}-lit-${theme}-${width}`);
          });
        }
      });
    }

    test("messages: a lit new sender's row offers 1 to 4 and the status bar spells them out", async ({
      page,
    }, testInfo) => {
      for (const width of WIDTHS) {
        await test.step(`${width}px`, async () => {
          await page.setViewportSize({
            width,
            height: width === 390 ? 844 : 900,
          });
          await openApp(page, "/messages");
          const noor = page.locator('li:has([data-testid="messages-row"])').filter({
            hasText: "Noor Haddad",
          });
          await expect(noor).toBeVisible();
          const before = await noor.boundingBox();
          await noor.getByTestId("messages-row").click();
          // A phone opens the person's page; back to the list, the row stays lit.
          if (width < 768) await page.keyboard.press("Escape");
          await expect(noor).toHaveAttribute("data-lit", "true");
          await expect(noor.getByTestId("new-sender-question").getByRole("button")).toHaveCount(4);
          await page.waitForTimeout(250);
          expect((await noor.boundingBox())?.height).toBe(before?.height);
          if (width >= 640) {
            await expect(page.getByTestId("status-reason")).toContainText(
              "New sender: 1 Messages · 2 Updates · 3 Reading · 4 Block",
            );
          }
          await attach(page, testInfo, `messages-newsender-${theme}-${width}`);
        });
      }
    });
  });
}

/** Touch emulation makes `(pointer: coarse)` true; Chrome ignores a media-feature override for it. */
test.describe("under a coarse pointer", () => {
  test.use({
    hasTouch: true,
    isMobile: true,
    viewport: { width: 1440, height: 900 },
  });

  for (const gate of KEY_PAGES) {
    test(`${gate.name}: no key chip shows`, async ({ page }) => {
      await openApp(page, gate.path);
      await expect(page.locator(gate.rows).first()).toBeVisible();
      await page.keyboard.press("j");
      await expect(page.locator("#main [data-lit='true']")).toHaveCount(1);
      await expect(page.locator("kbd:visible")).toHaveCount(0);
    });
  }
});

test.describe("the status strip and the keys", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  for (const gate of KEY_PAGES) {
    test(`${gate.name}: the status strip never cuts a word at 1024 with the assistant open`, async ({
      page,
    }) => {
      await page.setViewportSize({ width: 1024, height: 900 });
      await openApp(page, gate.path);
      await expect(page.locator(gate.rows).first()).toBeVisible();
      await page.getByRole("button", { name: "Assistant" }).click();
      await expect(page.getByRole("region", { name: "Assistant" })).toBeVisible();
      await page.keyboard.press("j");
      await expect(page.locator("#main [data-lit='true']")).toHaveCount(1);
      const strip = page.getByTestId("status-keys");
      await expect(strip.locator("li").first()).toBeAttached();
      // A strip with no room for even its first verb steps aside whole.
      const hidden = await strip.evaluate((node) => getComputedStyle(node).visibility === "hidden");
      if (hidden) return;
      // Each verb is wholly inside the strip or wholly dropped to the line it hides: never half cut.
      const cut = await strip.evaluate((node) => {
        const box = node.getBoundingClientRect();
        return [...node.querySelectorAll("li")]
          .filter((item) => {
            const rect = item.getBoundingClientRect();
            const dropped = rect.top >= box.bottom - 0.5;
            const whole = rect.left >= box.left - 0.5 && rect.right <= box.right + 0.5;
            return !dropped && !whole;
          })
          .map((item) => item.textContent);
      });
      expect(cut).toEqual([]);
      // The "?" is never dropped, and the reason shortens before any key does.
      await expect(page.getByRole("button", { name: /keys/ })).toBeVisible();
    });
  }

  test("the status strip changes per page", async ({ page }) => {
    const seen: string[] = [];
    for (const gate of KEY_PAGES) {
      await openApp(page, gate.path);
      await expect(page.locator(gate.rows).first()).toBeVisible();
      await page.keyboard.press("j");
      await expect(page.getByTestId("status-keys").locator("li").first()).toBeAttached();
      seen.push((await page.getByTestId("status-keys").textContent()) ?? "");
    }
    expect(new Set(seen).size).toBeGreaterThan(2);
  });
});

/** The states the W3 contact sheets need beyond a page's first look: a peek open, a reader open. */
for (const theme of THEMES) {
  test.describe(`${theme} theme: W3 states`, () => {
    test.beforeEach(async ({ page }) => {
      await page.addInitScript((chosen) => {
        localStorage.setItem(
          "mxr.uiPrefs",
          JSON.stringify({ state: { theme: chosen }, version: 6 }),
        );
      }, theme);
    });

    test("updates with a peek open, reading with its reader, a record's card", async ({
      page,
    }, testInfo) => {
      for (const width of WIDTHS) {
        await test.step(`${width}px`, async () => {
          await page.setViewportSize({
            width,
            height: width === 390 ? 844 : 900,
          });
          await openApp(page, "/updates");
          await expect(page.locator('[data-testid="update-line"]').first()).toBeVisible();
          await page.keyboard.press("Space");
          await expect(page.getByTestId("update-peek")).toBeVisible();
          await page.waitForTimeout(250);
          await attach(page, testInfo, `updates-peek-${theme}-${width}`);

          await openApp(page, "/reading");
          await expect(page.locator('[data-testid="reading-item"]').first()).toBeVisible();
          await page.keyboard.press("Enter");
          await page.waitForTimeout(600);
          await attach(page, testInfo, `reading-reader-${theme}-${width}`);

          await openApp(page, "/m/inbox");
          await expect(mailRows(page).first()).toBeVisible();
          await page.keyboard.press("Enter");
          await expect(reader(page)).toBeVisible();
          await page.waitForTimeout(400);
          await attach(page, testInfo, `inbox-reader-${theme}-${width}`);
        });
      }
    });
  });
}

/** W5: the map. The rail, the phone's More sheet, Settings, and two tool pages, in every theme. */
for (const theme of THEMES) {
  test.describe(`${theme} theme: W5 navigation`, () => {
    test.beforeEach(async ({ page }) => {
      await page.addInitScript((chosen) => {
        localStorage.setItem(
          "mxr.uiPrefs",
          JSON.stringify({ state: { theme: chosen }, version: 6 }),
        );
      }, theme);
    });

    test("the rail, expanded and collapsed, and the phone's More sheet", async ({
      page,
    }, testInfo) => {
      for (const width of WIDTHS) {
        await test.step(`${width}px`, async () => {
          await page.setViewportSize({
            width,
            height: width === 390 ? 844 : 900,
          });
          await openApp(page, "/updates");
          await expect(page.locator("#main")).toBeVisible();
          await page.evaluate(() => document.fonts.ready);
          if (width === 390) {
            await page.getByTestId("mobile-tabs").getByRole("button", { name: "More" }).click();
            await expect(page.getByTestId("more-sheet")).toBeVisible();
            await page.waitForTimeout(300);
            await attach(page, testInfo, `more-sheet-${theme}-${width}`);
            return;
          }
          const rail = page.getByRole("complementary", { name: "Mailboxes" });
          if (width === 1440) {
            // Expanded, with every group open: the dim tiers read against the page.
            for (const group of ["Mail", "Views", "Tools", "Labels"]) {
              await rail.getByRole("button", { name: group, exact: true }).click();
            }
            await page.waitForTimeout(250);
            await attach(page, testInfo, `rail-expanded-${theme}-${width}`);
            await page.getByRole("button", { name: "Collapse sidebar" }).click();
          }
          // Collapsed: the modes and Inbox, nothing else.
          await expect(rail.getByRole("link")).toHaveCount(7);
          await page.waitForTimeout(250);
          await attach(page, testInfo, `rail-collapsed-${theme}-${width}`);
        });
      }
    });

    test("settings, the phone's list, and two tool pages", async ({ page }, testInfo) => {
      for (const width of WIDTHS) {
        await test.step(`${width}px`, async () => {
          await page.setViewportSize({
            width,
            height: width === 390 ? 844 : 900,
          });
          const pages: [string, string][] = [
            ["settings", "/settings/llm"],
            ["subscriptions", "/subscriptions"],
            ["activity", "/activity"],
          ];
          if (width === 390) pages.unshift(["settings-list", "/settings/all"]);
          for (const [name, path] of pages) {
            await openApp(page, path);
            await expect(page.locator("#main h1:visible")).toHaveCount(1);
            await page.evaluate(() => document.fonts.ready);
            await page.waitForTimeout(400);
            // No sideways scroll, and a settings page has one nav, not two.
            const overflow = await page.evaluate(
              () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
            );
            expect(overflow, `${name} at ${width}px`).toBeLessThanOrEqual(0);
            if (name === "settings") {
              await expect(page.getByRole("navigation", { name: "Settings sections" })).toHaveCount(
                width >= 768 ? 1 : 0,
              );
            }
            await attach(page, testInfo, `${name}-${theme}-${width}`);
          }
        });
      }
    });

    test("screener, with a lit row and its choice", async ({ page }, testInfo) => {
      for (const width of WIDTHS) {
        await test.step(`${width}px`, async () => {
          await page.setViewportSize({
            width,
            height: width === 390 ? 844 : 900,
          });
          await openApp(page, "/screener");
          const rows = page.getByTestId("screener-row");
          await expect(rows.first()).toBeVisible();
          await expect(page.locator("#main [data-lit='true']")).toHaveCount(1);
          await page.waitForTimeout(300);
          const heights = await rows.evaluateAll((nodes) =>
            nodes.slice(0, 4).map((node) => node.getBoundingClientRect().height),
          );
          // Lighting never changes a row's height.
          expect(new Set(heights).size).toBe(1);
          await attach(page, testInfo, `screener-${theme}-${width}`);
        });
      }
    });
  });
}

/*
 * The copy desk (docs/blueprint/29): the assistant panel open beside Now and
 * Messages, in each state a conversation reaches, from a recorded
 * conversation. Looked at against the chapter's anatomy.
 */
const ASSISTANT_GATE_STATES = [
  "empty",
  "running",
  "finished",
  "proof",
  "question",
  "failure",
] as const;

const ASSISTANT_PAGES = [
  { name: "now", path: "/now" },
  { name: "messages", path: "/messages" },
] as const;

/** Tool names and raw JSON must never show by default (D135, D136). */
const PLUMBING = /mxr_[a-z_]+|[{]\s*"/;

for (const theme of THEMES) {
  test.describe(`${theme} theme: the copy desk`, () => {
    for (const gate of ASSISTANT_PAGES) {
      test(`the panel beside ${gate.name}, in every state`, async ({
        browser,
        baseURL,
      }, testInfo) => {
        // Eighteen pictures, each in a fresh context.
        test.setTimeout(300_000);
        for (const state of ASSISTANT_GATE_STATES) {
          for (const width of WIDTHS) {
            await test.step(`${state} at ${width}px`, async () => {
              // A fresh context per picture: each state brings its own stored panel and routes.
              const context = await browser.newContext({
                baseURL: baseURL ?? undefined,
                colorScheme: "dark",
                viewport: { width, height: width === 390 ? 844 : 900 },
              });
              try {
                const page = await context.newPage();
                await page.addInitScript((chosen) => {
                  localStorage.setItem(
                    "mxr.uiPrefs",
                    JSON.stringify({ state: { theme: chosen }, version: 6 }),
                  );
                }, theme);
                await mockAssistant(page, state);
                await openApp(page, gate.path);
                const panel = page.getByRole("region", { name: "Assistant" });
                await expect(panel).toBeVisible();
                await expect(panel.getByLabel("Message the assistant")).toBeVisible();
                await page.evaluate(() => document.fonts.ready);
                if (state === "running") {
                  await expect(panel.getByRole("status")).toContainText("Reading your request");
                }
                if (state === "finished") {
                  await expect(panel.getByTestId("model-mark")).toBeVisible();
                }
                await page.waitForTimeout(300);
                await attach(page, testInfo, `assistant-${state}-${gate.name}-${theme}-${width}`);
                if (state === "finished") {
                  await panel.getByRole("button", { name: /\d steps?/ }).click();
                  await panel.getByRole("button", { name: /Searched your mail/ }).click();
                  await page.waitForTimeout(200);
                  await attach(page, testInfo, `assistant-steps-${gate.name}-${theme}-${width}`);
                }
              } finally {
                await context.close();
              }
            });
          }
        }
      });
    }
  });
}

test.describe("the copy desk's rules", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
  });

  test("a reply and its folded steps show no tool name or JSON; Details has them", async ({
    page,
  }) => {
    await mockAssistant(page, "finished");
    await openApp(page, "/now");
    const panel = page.getByRole("region", { name: "Assistant" });
    await expect(panel.getByTestId("model-mark")).toBeVisible();
    // The reply's rows and chips are there; the plumbing is not.
    await expect(
      panel.getByRole("list", { name: "The emails and people in this reply" }),
    ).toBeVisible();
    expect(await panel.innerText()).not.toMatch(PLUMBING);
    await panel.getByRole("button", { name: /steps/ }).click();
    await panel.getByRole("button", { name: /Searched your mail/ }).click();
    expect(await panel.innerText()).not.toMatch(PLUMBING);
    await panel
      .getByRole("button", { name: /Details/ })
      .first()
      .click();
    expect(await panel.innerText()).toMatch(/mxr_search/);
  });

  test("a proof has one primary button; y signs it; the card gives way to its wire line", async ({
    page,
  }) => {
    let decided: unknown = null;
    await mockAssistant(page, "proof", (body) => {
      decided = body;
    });
    await openApp(page, "/now");
    const panel = page.getByRole("region", { name: "Assistant" });
    const proof = panel.getByRole("region", { name: "Archive 14 emails" });
    await expect(proof).toBeVisible();
    expect(await proof.innerText()).not.toMatch(PLUMBING);
    await expect(proof.getByText("CI failed on main: build-linux")).toBeVisible();
    // Exactly one filled (primary) button on the proof.
    const filled = await proof
      .getByRole("button")
      .evaluateAll((nodes) => nodes.filter((node) => node.className.includes("bg-primary")).length);
    expect(filled).toBe(1);

    // Typing in the composer never answers the proof.
    const composer = panel.getByLabel("Message the assistant");
    await composer.click();
    await page.keyboard.type("ynx");
    expect(decided).toBeNull();
    await expect(composer).toHaveValue("ynx");

    // Esc hands the keyboard to the proof; y signs it.
    await composer.fill("");
    await page.keyboard.press("Escape");
    await expect(proof).toBeFocused();
    await page.keyboard.press("y");
    await expect
      .poll(() => decided)
      .toEqual({
        decisions: [{ call_id: "archive", decision: "approve" }],
      });
  });

  test("a question takes the keys 1 to 6 and Enter", async ({ page }) => {
    let decided: unknown = null;
    await mockAssistant(page, "question", (body) => {
      decided = body;
    });
    await openApp(page, "/now");
    const card = page.getByRole("region", { name: "A question for you" });
    await expect(card.getByText("recommended")).toBeVisible();
    await card.focus();
    await page.keyboard.press("1");
    await expect(card.getByRole("radio", { name: /Work/ })).toHaveAttribute("aria-checked", "true");
    await page.keyboard.press("Enter");
    await expect
      .poll(() => decided)
      .toEqual({
        decisions: [{ call_id: "ask", decision: "answer", answers: [{ selected: ["Work"] }] }],
      });
  });
});
