import { expect, test, type Page } from "@playwright/test";

import { bridge, openApp } from "./helpers/state";

test.use({ viewport: { width: 1440, height: 900 } });

interface Edition {
  bands: { items: { item_key: string; title: string }[] }[];
}

async function firstItem(page: Page): Promise<{ item_key: string; title: string }> {
  let found: { item_key: string; title: string } | undefined;
  await expect
    .poll(
      async () => {
        const { edition } = await bridge<{ edition: Edition }>(page, "/api/v1/mail/reading");
        found = edition.bands.flatMap((band) => band.items)[0];
        return found !== undefined;
      },
      { timeout: 60_000 },
    )
    .toBe(true);
  return found!;
}

const HOSTILE_HTML =
  `<h2>Hostile heading</h2><p onclick="window.__pwned = 1">Hostile paragraph</p>` +
  `<script>window.__pwned = 2</script><style>body{display:none}</style>` +
  `<a href="javascript:window.__pwned=3">js link</a><a href="https://example.com/safe">safe link</a>` +
  `<img src="https://tracker.example/pixel.png?id=1" alt="remote" onerror="window.__pwned=4">` +
  `<iframe src="https://evil.example"></iframe><form action="https://evil.example"><input name="q"></form>` +
  `<div style="position:fixed;inset:0;z-index:9999;background:red" class="fixed inset-0">overlay</div>`;

test("an item with HTML reads as an article, and the reader settings stay after a reload", async ({
  page,
}) => {
  const { item_key } = await firstItem(page);
  await openApp(page, `/reading/item/${encodeURIComponent(item_key)}`);
  const text = page.getByTestId("reader-text");
  await expect(text).toHaveAttribute("data-format", "html");
  const column = page.getByTestId("reader-column");
  const bodyFontSize = () =>
    text.locator("[data-article-body]").evaluate((node) => getComputedStyle(node).fontSize);
  expect(await bodyFontSize()).toBe("20px");

  await page.getByTestId("reader-settings").click();
  await page.getByRole("button", { name: "Larger text" }).click();
  await page.getByRole("button", { name: "Larger text" }).click();
  await page.getByRole("radio", { name: "Sepia" }).click();
  await page.keyboard.press("Escape");
  await expect.poll(bodyFontSize).toBe("22px");
  await expect(column).toHaveAttribute("data-reader-theme", "sepia");

  await page.reload();
  await expect(page.getByTestId("reader-text")).toHaveAttribute("data-format", "html");
  await expect.poll(bodyFontSize).toBe("22px");
  await expect(page.getByTestId("reader-column")).toHaveAttribute("data-reader-theme", "sepia");
});

test("Later, Let go and Unsubscribe each appear once in the reader", async ({ page }) => {
  const { item_key } = await firstItem(page);
  await openApp(page, `/reading/item/${encodeURIComponent(item_key)}`);
  await expect(page.getByTestId("reader-text")).toBeVisible();
  for (const name of ["Later", "Let go", "Unsubscribe"]) {
    await expect(page.getByRole("button", { name: new RegExp(`^${name}`) })).toHaveCount(1);
  }
  // The end of the article keeps what the toolbar doesn't have.
  await expect(page.getByTestId("end-of-issue").getByRole("button")).toHaveText(/^Next/);
});

test("hostile article HTML renders inert and holds remote images back", async ({ page }) => {
  const { item_key } = await firstItem(page);
  await page.route("**/api/v1/mail/reading/items/**", async (route) => {
    const response = await route.fetch();
    const json = await response.json();
    json.item.html = HOSTILE_HTML;
    await route.fulfill({ response, json });
  });
  await openApp(page, `/reading/item/${encodeURIComponent(item_key)}`);
  const text = page.getByTestId("reader-text");
  await expect(text.getByRole("heading", { name: "Hostile heading" })).toBeVisible();

  // SAFETY: reads a global the hostile markup would have set, if it had run.
  expect(
    await page.evaluate(() => (window as unknown as { __pwned?: number }).__pwned),
  ).toBeUndefined();
  await expect(text.locator("script, style, iframe, form, input, img")).toHaveCount(0);
  await expect(text.locator("[onclick], [onerror]")).toHaveCount(0);
  // The only styles and classes left are the reader's own: paragraph indent variables and the
  // blocked-image note, the renderer's wrappers. Nothing from the sender (position, background, "fixed inset-0").
  const own = await text.evaluate((node) => ({
    styles: [...node.querySelectorAll("[style]")].map((el) => el.getAttribute("style") ?? ""),
    classes: [...node.querySelectorAll("[class]")].map((el) => el.getAttribute("class") ?? ""),
  }));
  expect(own.styles.every((style) => /^--para-indent: [\d.]+px;?$/.test(style.trim()))).toBe(true);
  expect(own.classes.sort()).toEqual(["mxr-image-blocked", "reader-article", "reader-prose"]);
  await expect(text.getByText("Image blocked")).toBeVisible();
  await expect(text.getByRole("link", { name: "safe link" })).toHaveAttribute("target", "_blank");
  await expect(text.getByText("js link")).not.toHaveAttribute("href", /.+/);
  // The overlay div's text survives as plain text, but it cannot cover the app.
  await expect(page.getByTestId("reading-reader")).toBeVisible();
  await expect(page.getByRole("button", { name: "Reader settings" })).toBeInViewport();

  await page.getByRole("button", { name: "Show images" }).click();
  await expect(text.locator("img")).toHaveAttribute("src", /tracker\.example/);
});
