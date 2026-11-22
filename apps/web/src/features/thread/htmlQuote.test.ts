/* @vitest-environment jsdom */

import { describe, expect, test } from "vitest";

import { splitHtmlQuote } from "./htmlQuote";

describe("splitHtmlQuote", () => {
  test("drops a Gmail quote and keeps the reply", () => {
    const html = `<div dir="ltr">Works for me.</div><br><div class="gmail_quote"><div class="gmail_attr">On Mon Ada wrote:</div><blockquote>Old text</blockquote></div>`;
    const parts = splitHtmlQuote(html);
    expect(parts.hasQuote).toBe(true);
    expect(parts.main).toContain("Works for me.");
    expect(parts.main).not.toContain("Old text");
  });
});
