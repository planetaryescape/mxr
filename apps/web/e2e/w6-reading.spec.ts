import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Locator, type Page } from "@playwright/test";

import { animationsSettled, mailRows, openList, reader } from "./helpers/mail";
import { bridge, openApp } from "./helpers/state";

/*
 * Rung W6: when I read, the email is the page; when I write, the editor is
 * the page. The behaviour checks run at 1440px; the gate test screenshots
 * the reader, Reading's reader, compose and search in every theme at three
 * widths (MXR_DESIGN_GATE_DIR also writes them to a folder).
 */

const THEMES = ["midnight", "light", "paper", "eclipse"] as const;
const WIDTHS = [390, 1024, 1440] as const;

/** A table-layout newsletter, the way senders still write them. */
const NEWSLETTER = `<table width="600" align="center" cellpadding="0" cellspacing="0" style="margin:0 auto;background:#ffffff"><tr><td style="padding:28px 32px;font-family:Georgia,serif;color:#222"><h1 style="font-size:28px;margin:0 0 12px">This Week in Rust</h1><p style="font-size:16px;line-height:1.6">Hello Rustaceans. This is a long paragraph of newsletter prose that keeps going, so a real newsletter has to wrap inside the column the page gives it and still look like the sender's own work.</p><p style="font-size:16px;line-height:1.6">A second paragraph follows, with <a href="https://example.com/a" style="color:#0b5fff">a link to an article</a> and enough words to wrap onto several lines at any width.</p><hr><p style="font-size:13px;color:#777">You are receiving this because you subscribed.</p></td></tr></table>`;

/** The width of `locator` in ch of its own font: how many zeros fit on a line. */
async function widthInCh(locator: Locator): Promise<number> {
  return locator.evaluate((element) => {
    const probe = document.createElement("span");
    probe.textContent = "0";
    probe.style.cssText = "position:absolute;visibility:hidden";
    element.appendChild(probe);
    const zero = probe.getBoundingClientRect().width;
    probe.remove();
    return element.getBoundingClientRect().width / zero;
  });
}

async function firstReadingKey(page: Page): Promise<string> {
  const { edition } = await bridge<{ edition: { bands: { items: { item_key: string }[] }[] } }>(
    page,
    "/api/v1/mail/reading",
  );
  const item = edition.bands.flatMap((band) => band.items)[0];
  if (!item) throw new Error("the fixture has no Reading items");
  return item.item_key;
}

async function mockNewsletterIssue(page: Page): Promise<void> {
  await page.route("**/api/v1/mail/reading/items/**", async (route) => {
    const response = await route.fetch();
    const json = await response.json();
    json.item.html = NEWSLETTER;
    await route.fulfill({ response, json });
  });
}

/** Opens the fixture thread with five messages, the long one. */
async function openLongThread(page: Page): Promise<void> {
  await openList(page, "/m/inbox");
  await mailRows(page)
    .filter({ hasText: /Re: Security review follow-up/ })
    .first()
    .click();
  await expect(reader(page)).toBeVisible();
}

test.describe("at 1440px", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("a long thread reads as a page: the reading voice at 64 to 70ch, earlier messages folded", async ({
    page,
  }) => {
    await openLongThread(page);
    // The fixture's mail is HTML; the reader view is the page set in the reading voice.
    await reader(page).getByRole("radio", { name: "Reader" }).click();
    const text = reader(page).getByTestId("message-text").first();
    await expect(text).toBeVisible();
    const ch = await widthInCh(text);
    expect(ch, `the reading measure is ${ch}ch`).toBeGreaterThanOrEqual(64);
    expect(ch, `the reading measure is ${ch}ch`).toBeLessThanOrEqual(70);
    // The body is in the reading voice: 16px on 26px.
    await expect(text).toHaveCSS("font-size", "16px");
    await expect(text).toHaveCSS("line-height", "26px");
    // Earlier messages are one line each; the newest is open.
    expect(await reader(page).locator('[data-collapsed="true"]').count()).toBeGreaterThanOrEqual(3);
    // One page title, in Display.
    await expect(reader(page).getByRole("heading", { level: 1 })).toHaveCSS("font-size", "28px");
  });

  test("the reader and Reading's reader share one toolbar style", async ({ page }) => {
    await openLongThread(page);
    const style = (button: Locator) =>
      button.evaluate((element) => {
        const css = getComputedStyle(element);
        return {
          height: css.height,
          color: css.color,
          fontSize: css.fontSize,
          border: css.borderTopWidth,
          background: css.backgroundColor,
          radius: css.borderTopLeftRadius,
        };
      });
    const archive = await style(reader(page).getByRole("button", { name: "Archive" }));
    const key = await firstReadingKey(page);
    await openApp(page, `/reading/item/${encodeURIComponent(key)}`);
    const later = await style(page.getByRole("button", { name: "Later", exact: true }).first());
    expect(later).toEqual(archive);
  });

  test("a newsletter in Reading shares one left edge with its title and holds the measure", async ({
    page,
  }) => {
    await mockNewsletterIssue(page);
    await openApp(page, `/reading/item/${encodeURIComponent(await firstReadingKey(page))}`);
    const body = page.getByTestId("reader-text").locator("[data-article-body]");
    await expect(body).toBeVisible();
    await animationsSettled(page);
    const title = await page.getByTestId("reading-reader").locator("header h1").boundingBox();
    const column = await body.boundingBox();
    expect(Math.abs((title?.x ?? 0) - (column?.x ?? 1000))).toBeLessThanOrEqual(1);
    // The toolbar sits in the same column, on one row: its back arrow starts
    // where the title does.
    const toolbar = page.getByTestId("reading-reader").locator("header");
    const back = toolbar.getByRole("button", { name: "Reading", exact: true }).locator("svg");
    const arrow = await back.boundingBox();
    expect(Math.abs((arrow?.x ?? 1000) - (title?.x ?? 0))).toBeLessThanOrEqual(1);
    const later = await toolbar.getByRole("button", { name: "Later", exact: true }).boundingBox();
    expect(Math.abs((later?.y ?? 1000) - (arrow?.y ?? 0))).toBeLessThanOrEqual(8);
    const ch = await widthInCh(body);
    expect(ch).toBeGreaterThanOrEqual(64);
    expect(ch).toBeLessThanOrEqual(70);
    // The sender's table keeps its own layout inside the column and never pushes past it.
    const table = await page.getByTestId("reader-text").locator("table").boundingBox();
    expect(table?.width ?? 0).toBeLessThanOrEqual((column?.width ?? 0) + 1);
    // Reading to the end ends on the issue's own verbs.
    await expect(page.getByTestId("end-of-issue")).toBeAttached();
  });

  test("a newsletter in a thread keeps its sender's canvas inside the reader", async ({ page }) => {
    await openLongThread(page);
    await page.route("**/api/v1/mail/threads/*", async (route) => {
      const response = await route.fetch();
      const json = await response.json();
      for (const body of json.bodies) body.text_html = NEWSLETTER;
      await route.fulfill({ response, json });
    });
    await page.reload();
    const frame = reader(page).locator("iframe[sandbox]").first();
    await expect(frame).toBeVisible();
    const box = await frame.boundingBox();
    const column = await reader(page).getByTestId("thread-scroll").boundingBox();
    expect(box?.width ?? 0).toBeLessThanOrEqual(column?.width ?? 0);
    await expect(frame.contentFrame().getByText("This Week in Rust")).toBeVisible();
  });

  test("compose is a sheet over a scrim, with a Title and the vim line only in vim mode", async ({
    page,
  }) => {
    await openList(page, "/m/inbox");
    await page.keyboard.press("c");
    const sheet = page.getByRole("dialog", { name: "New message" });
    await expect(sheet).toBeVisible();
    await expect(page.getByRole("heading", { name: "New message", level: 2 })).toHaveCSS(
      "font-size",
      "20px",
    );
    // The rows behind are dimmed by a scrim that covers the page.
    const scrim = page.getByTestId("compose-scrim");
    expect(await scrim.boundingBox()).toEqual({ x: 0, y: 0, width: 1440, height: 900 });
    // The editor mode is the shared segmented control.
    const mode = page.getByRole("radiogroup", { name: "Editor mode" });
    await expect(mode.getByRole("radio", { name: "Markdown" })).toBeChecked();
    await expect(page.getByTestId("vim-mode")).toBeVisible();
    await mode.getByRole("radio", { name: "Rich text" }).click();
    await expect(page.getByTestId("vim-mode")).toHaveCount(0);
    // Send is the sheet's one filled button.
    const filled = await sheet.getByRole("button").evaluateAll(
      (buttons) =>
        buttons.filter((button) => {
          const background = getComputedStyle(button).backgroundColor;
          return background !== "rgba(0, 0, 0, 0)" && background !== "transparent";
        }).length,
    );
    expect(filled).toBeGreaterThanOrEqual(1);
    await expect(sheet.getByRole("button", { name: /^Send(?! later)/ })).toBeVisible();
  });

  test("search results are titled by the query and keep their filters behind g f", async ({
    page,
  }) => {
    await openList(page, "/search?q=review");
    await expect(page.getByRole("heading", { name: "review", level: 1 })).toBeVisible();
    await expect(page.getByText(/^\d+ conversations?$/)).toBeVisible();
    await expect(page.getByRole("radiogroup", { name: "Search mode" })).toBeVisible();
    await expect(page.getByTestId("search-filters")).toHaveCount(0);
    await page.keyboard.press("g");
    await page.keyboard.press("f");
    await expect(page.getByTestId("search-filters")).toBeVisible();
    await page.keyboard.press("g");
    await page.keyboard.press("f");
    await expect(page.getByTestId("search-filters")).toHaveCount(0);
  });
});

async function snap(page: Page, label: string): Promise<void> {
  const body = await page.screenshot();
  test.info().attachments.push({ name: label, body, contentType: "image/png" });
  const dir = process.env.MXR_DESIGN_GATE_DIR;
  if (dir) {
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, `${label}.png`), body);
  }
}

for (const theme of THEMES) {
  test(`${theme}: the reader, Reading's reader, compose and search at three widths`, async ({
    page,
  }) => {
    test.setTimeout(240_000);
    await page.addInitScript((chosen) => {
      localStorage.setItem("mxr.uiPrefs", JSON.stringify({ state: { theme: chosen }, version: 6 }));
    }, theme);
    await mockNewsletterIssue(page);
    for (const width of WIDTHS) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
      const settle = async () => {
        await page.evaluate(() => document.fonts.ready);
        await page.waitForTimeout(500);
      };

      await openList(page, "/m/inbox");
      await mailRows(page)
        .filter({ hasText: /Re: Security review follow-up/ })
        .first()
        .click();
      await expect(reader(page)).toBeVisible();
      await settle();
      await snap(page, `w6-thread-${theme}-${width}`);

      await page.keyboard.press("r");
      await expect(page.locator("[data-compose-surface]")).toBeVisible();
      await settle();
      await snap(page, `w6-compose-reply-${theme}-${width}`);
      await page.getByRole("button", { name: "Close composer (saves draft)" }).click();

      await openApp(page, `/reading/item/${encodeURIComponent(await firstReadingKey(page))}`);
      await expect(page.getByTestId("reader-text")).toBeVisible();
      await settle();
      await snap(page, `w6-reading-${theme}-${width}`);

      await openList(page, "/m/inbox");
      await page.keyboard.press("c");
      await expect(page.getByRole("dialog", { name: "New message" })).toBeVisible();
      await settle();
      await snap(page, `w6-compose-new-${theme}-${width}`);
      await page.getByRole("button", { name: "Close composer (saves draft)" }).click();

      await openList(page, "/search?q=review");
      await settle();
      await snap(page, `w6-search-${theme}-${width}`);
    }
  });
}
