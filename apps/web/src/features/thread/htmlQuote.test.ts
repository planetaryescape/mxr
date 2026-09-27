/* @vitest-environment jsdom */

import { describe, expect, test } from "vitest";

import { countRemoteImages, splitHtmlQuote } from "./htmlQuote";

describe("splitHtmlQuote", () => {
  test("drops a Gmail quote and keeps the reply", () => {
    const html = `<div dir="ltr">Works for me.</div><br><div class="gmail_quote"><div class="gmail_attr">On Mon Ada wrote:</div><blockquote>Old text</blockquote></div>`;
    const parts = splitHtmlQuote(html);
    expect(parts.hasQuote).toBe(true);
    expect(parts.main).toContain("Works for me.");
    expect(parts.main).not.toContain("Old text");
  });

  test("drops everything after an Outlook reply header", () => {
    const html = `<div>New reply</div><hr><div id="divRplyFwdMsg"><b>From:</b> Grace</div><div>Earlier message</div>`;
    const parts = splitHtmlQuote(html);
    expect(parts.hasQuote).toBe(true);
    expect(parts.main).toContain("New reply");
    expect(parts.main).not.toContain("Earlier message");
    expect(parts.main).not.toContain("Grace");
  });

  test("removes a Gmail signature and reports it", () => {
    const html = `<p>Hi</p><div class="gmail_signature">Ada | Engines Ltd</div>`;
    const parts = splitHtmlQuote(html);
    expect(parts.hasSignature).toBe(true);
    expect(parts.main).not.toContain("Engines Ltd");
  });

  test("keeps a bare forward whole rather than rendering nothing", () => {
    const html = `<div class="gmail_quote">---------- Forwarded message ---------<br>Body</div>`;
    const parts = splitHtmlQuote(html);
    expect(parts.hasQuote).toBe(false);
    expect(parts.main).toBe(html);
  });

  test("leaves a message without markers unchanged", () => {
    const parts = splitHtmlQuote("<p>Plain</p>");
    expect(parts).toEqual({ main: "<p>Plain</p>", hasQuote: false, hasSignature: false });
  });
});

describe("countRemoteImages", () => {
  test("counts http(s) images, not inline cid or data images", () => {
    const html = `<img src="https://cdn.example.com/a.png"><img src="cid:logo"><img src="http://x.test/b.gif">`;
    expect(countRemoteImages(html)).toBe(2);
  });
});
