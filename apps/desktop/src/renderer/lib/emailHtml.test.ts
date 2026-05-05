import { describe, expect, it } from "vitest";
import {
  buildPlainTextEmailDocument,
  buildSanitizedEmailDocument,
  sanitizeEmailHtml,
} from "./emailHtml";

describe("email HTML sanitization", () => {
  it("removes scripts, forms, iframe content, and event handlers", () => {
    const html = sanitizeEmailHtml(`
      <p onclick="alert(1)">Hello <strong>world</strong></p>
      <script>alert(1)</script>
      <form action="https://evil.test"><input name="token" /></form>
      <iframe src="https://evil.test"></iframe>
      <img src="data:image/png;base64,abc" onerror="alert(1)" alt="chart">
    `);

    expect(html).toContain("<strong>world</strong>");
    expect(html).toContain('src="data:image/png;base64,abc"');
    expect(html).not.toMatch(/script|form|iframe|input|onclick|onerror/i);
  });

  it("blocks remote loads by default", () => {
    const html = sanitizeEmailHtml(`
      <img src="https://cdn.example.com/pixel.png" srcset="https://cdn.example.com/a.png 1x">
      <a href="https://example.com">Remote link</a>
      <div style="color: red; background-image: url(https://cdn.example.com/bg.png)">Styled</div>
    `);

    expect(html).not.toContain("https://");
    expect(html).not.toContain("srcset=");
    expect(html).not.toContain("background-image");
    expect(html).toContain("Remote link");
    expect(html).toContain("Styled");
  });

  it("allows remote reader content only when explicitly requested", () => {
    const html = sanitizeEmailHtml(
      '<img src="https://cdn.example.com/pixel.png" onload="alert(1)"><script>alert(1)</script>',
      { allowRemoteContent: true },
    );

    expect(html).toContain('src="https://cdn.example.com/pixel.png"');
    expect(html).not.toMatch(/script|onload/i);
  });

  it("builds offline browser documents from sanitized HTML", () => {
    const documentHtml = buildSanitizedEmailDocument({
      title: 'Subject <img src=x onerror="alert(1)">',
      html: '<p>Safe</p><img src="https://cdn.example.com/pixel.png"><script>alert(1)</script>',
    });

    expect(documentHtml).toContain("Content-Security-Policy");
    expect(documentHtml).toContain("script-src &#39;none&#39;");
    expect(documentHtml).toContain("Subject &lt;img src=x onerror=&quot;alert(1)&quot;&gt;");
    expect(documentHtml).toContain("<p>Safe</p>");
    expect(documentHtml).not.toContain("https://cdn.example.com");
    expect(documentHtml).not.toMatch(/<script/i);
  });

  it("escapes plain text browser documents", () => {
    const documentHtml = buildPlainTextEmailDocument("Plain", "<b>not html</b>");

    expect(documentHtml).toContain("&lt;b&gt;not html&lt;/b&gt;");
    expect(documentHtml).not.toContain("<b>not html</b>");
  });
});
