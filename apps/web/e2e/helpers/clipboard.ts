import type { BrowserContext, Page } from "@playwright/test";

/*
 * Playwright's WebKit has no "clipboard-read" or "clipboard-write" permission
 * (grantPermissions throws "Unknown permission"), so the clipboard can't be
 * read back there. On WebKit the page records each writeText call and still
 * forwards it, so the app's own copy path runs; Chromium reads the real clipboard.
 */

const isWebkit = (context: BrowserContext) => context.browser()?.browserType().name() === "webkit";

export async function allowClipboard(context: BrowserContext): Promise<void> {
  if (!isWebkit(context)) {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    return;
  }
  await context.addInitScript(() => {
    const copied: string[] = [];
    (window as unknown as { __copied: string[] }).__copied = copied;
    const original = navigator.clipboard.writeText.bind(navigator.clipboard);
    navigator.clipboard.writeText = (text: string) => {
      copied.push(text);
      return original(text);
    };
  });
}

export function readClipboard(page: Page): Promise<string> {
  return page.evaluate(async () => {
    const copied = (window as unknown as { __copied?: string[] }).__copied;
    return copied ? (copied.at(-1) ?? "") : navigator.clipboard.readText();
  });
}
