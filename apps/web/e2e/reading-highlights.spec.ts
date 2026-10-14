import { expect, test, type Page } from "@playwright/test";

import { bridge, bridgeAuth, openApp } from "./helpers/state";
import { readE2EState } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface Edition {
  bands: { items: { item_key: string }[] }[];
}
interface Detail {
  item: {
    highlights: { id: string; quote: string; note?: string | null; paragraph?: number | null }[];
  };
}

async function firstItemKey(page: Page): Promise<string> {
  let key: string | undefined;
  await expect
    .poll(
      async () => {
        const { edition } = await bridge<{ edition: Edition }>(page, "/api/v1/mail/reading");
        key = edition.bands.flatMap((band) => band.items)[0]?.item_key;
        return key !== undefined;
      },
      { timeout: 60_000 },
    )
    .toBe(true);
  return key!;
}

const highlightsOf = async (page: Page, key: string) =>
  (await bridge<Detail>(page, `/api/v1/mail/reading/items/${encodeURIComponent(key)}`)).item
    .highlights;

/** Selects the first sentence-ish run of the article's first paragraph. */
async function selectInArticle(page: Page): Promise<string> {
  return page.getByTestId("reader-text").evaluate((root) => {
    const paragraph = root.querySelector("[data-article-body] p") ?? root.querySelector("p");
    if (!paragraph) throw new Error("no paragraph");
    const walker = document.createTreeWalker(paragraph, NodeFilter.SHOW_TEXT);
    const node = walker.nextNode() as Text;
    const end = Math.min(node.data.length, 40);
    const range = document.createRange();
    range.setStart(node, 0);
    range.setEnd(node, end);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    return selection.toString();
  });
}

test("select, highlight with a note, edit it, remove it, and undo brings it back", async ({
  page,
}) => {
  const key = await firstItemKey(page);
  await openApp(page, `/reading/item/${encodeURIComponent(key)}`);
  await expect(page.getByTestId("reader-text")).toBeVisible();
  const before = (await highlightsOf(page, key)).length;

  const quote = await selectInArticle(page);
  const toolbar = page.getByTestId("highlight-toolbar");
  await expect(toolbar).toBeVisible();
  await page.screenshot({
    path: process.env.R3_SHOTS ? `${process.env.R3_SHOTS}/toolbar.png` : undefined,
  });
  await page.getByTestId("toolbar-highlight-note").click();
  await page.getByTestId("highlight-note-input").fill("for the talk");
  await page.screenshot({
    path: process.env.R3_SHOTS ? `${process.env.R3_SHOTS}/toolbar-note.png` : undefined,
  });
  await page.getByTestId("highlight-note-input").press("Enter");

  const list = page.getByTestId("reading-highlights");
  await expect(list).toContainText("for the talk");
  const saved = (await highlightsOf(page, key)).at(-1)!;
  expect(saved.note).toBe("for the talk");
  expect(saved.quote.replace(/\s+/g, " ")).toContain(quote.replace(/\s+/g, " ").trim());

  // The list row edits the note; a blank note clears it.
  const row = page.getByTestId("reading-highlight").last();
  await row.getByTestId("highlight-edit-note").click();
  await row.getByTestId("highlight-note-input").fill("changed");
  await page.screenshot({
    path: process.env.R3_SHOTS ? `${process.env.R3_SHOTS}/edit-note.png` : undefined,
  });
  await row.getByTestId("highlight-note-input").press("Enter");
  await expect(list).toContainText("changed");
  await row.getByTestId("highlight-edit-note").click();
  await row.getByTestId("highlight-note-input").fill("");
  await row.getByTestId("highlight-note-input").press("Enter");
  await expect.poll(async () => (await highlightsOf(page, key)).at(-1)?.note ?? null).toBeNull();

  // Remove, then undo with u.
  await row.getByTestId("highlight-remove").click();
  await expect.poll(async () => (await highlightsOf(page, key)).length).toBe(before);
  await page.screenshot({
    path: process.env.R3_SHOTS ? `${process.env.R3_SHOTS}/removed.png` : undefined,
  });
  await page.keyboard.press("u");
  await expect.poll(async () => (await highlightsOf(page, key)).length).toBe(before + 1);
  expect((await highlightsOf(page, key)).at(-1)?.id).toBe(saved.id);
});

/**
 * Selects the start of the last text paragraph and returns its index as the
 * reader numbers it (`data-paragraph` in the plain text, one-based
 * `data-para` in the HTML article).
 */
async function selectInLastParagraph(page: Page): Promise<{ text: string; index: number }> {
  return page.getByTestId("reader-text").evaluate((root) => {
    const tagged = [...root.querySelectorAll<HTMLElement>("[data-paragraph], [data-para]")].filter(
      (el) => (el.textContent ?? "").trim().length > 20,
    );
    const last = tagged.at(-1);
    if (!last) throw new Error("no paragraph");
    const index =
      last.dataset.paragraph !== undefined
        ? Number(last.dataset.paragraph)
        : Number(last.dataset.para) - 1;
    const walker = document.createTreeWalker(last, NodeFilter.SHOW_TEXT);
    const node = walker.nextNode() as Text;
    const range = document.createRange();
    range.setStart(node, 0);
    range.setEnd(node, Math.min(node.data.length, 20));
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    return { text: selection.toString(), index };
  });
}

test("a highlight remembers the paragraph it was selected in", async ({ page }) => {
  const key = await firstItemKey(page);
  await openApp(page, `/reading/item/${encodeURIComponent(key)}`);
  await expect(page.getByTestId("reader-text")).toBeVisible();
  const before = (await highlightsOf(page, key)).length;

  const { index } = await selectInLastParagraph(page);
  await page.getByTestId("toolbar-highlight").click();
  await expect.poll(async () => (await highlightsOf(page, key)).length).toBe(before + 1);
  expect((await highlightsOf(page, key)).at(-1)?.paragraph).toBe(index);

  // Reloaded, the mark is drawn once, in the paragraph that was selected.
  await page.reload();
  await expect(page.getByTestId("reader-text")).toBeVisible();
  const saved = (await highlightsOf(page, key)).at(-1)!;
  expect(saved.paragraph).toBe(index);
});

test("a selection in the highlights list does not show the toolbar", async ({ page }) => {
  const key = await firstItemKey(page);
  await page.request.post(`${readE2EState().bridgeUrl}/api/v1/mail/reading/highlights`, {
    headers: bridgeAuth(),
    data: { item_key: key, quote: "list only", view: "issue" },
  });
  await openApp(page, `/reading/item/${encodeURIComponent(key)}`);
  const row = page.getByTestId("reading-highlight").first();
  await expect(row).toBeVisible();
  await row.evaluate((node) => {
    const range = document.createRange();
    range.selectNodeContents(node);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
  });
  await expect(page.getByTestId("highlight-toolbar")).toHaveCount(0);
});

test("the bridge previews a removal without removing, and refuses a null note", async ({
  page,
}) => {
  const key = await firstItemKey(page);
  const url = readE2EState().bridgeUrl;
  const saved = await page.request.post(`${url}/api/v1/mail/reading/highlights`, {
    headers: bridgeAuth(),
    data: { item_key: key, quote: "preview me", view: "issue", note: "keep" },
  });
  const { highlight } = (await saved.json()) as { highlight: { id: string } };

  const nullNote = await page.request.post(
    `${url}/api/v1/mail/reading/highlights/${highlight.id}/note`,
    {
      headers: bridgeAuth(),
      data: { note: null },
    },
  );
  expect(nullNote.ok()).toBe(false);
  expect((await highlightsOf(page, key)).find((h) => h.id === highlight.id)?.note).toBe("keep");

  const preview = await page.request.post(
    `${url}/api/v1/mail/reading/highlights/${highlight.id}/remove`,
    { headers: bridgeAuth(), data: { dry_run: true } },
  );
  const body = (await preview.json()) as {
    change: { dry_run: boolean; highlight: { id: string } };
  };
  expect(body.change.dry_run).toBe(true);
  expect(body.change.highlight.id).toBe(highlight.id);
  expect((await highlightsOf(page, key)).some((h) => h.id === highlight.id)).toBe(true);
});
