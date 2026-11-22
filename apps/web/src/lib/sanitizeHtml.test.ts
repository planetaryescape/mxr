import { describe, expect, it } from "vitest";

import { sanitizeHtml, sanitizeReaderHtml } from "./sanitizeHtml";

describe("sanitizeHtml", () => {
  it("strips inline <script> tags", () => {
    const dirty = `<p>hi</p><script>window.X=1</script>`;
    const clean = sanitizeHtml(dirty);

    expect(clean).not.toMatch(/<script/i);
    expect(clean).not.toMatch(/window\.X/);
  });

  it("removes dangerous CSS from style attributes", () => {
    const dirty = `<p style="color:#777;background:url(http://evil/leak);position:fixed">hi</p>`;
    const clean = sanitizeHtml(dirty);

    expect(clean).toMatch(/style="color: #777"/);
    expect(clean).not.toMatch(/evil/);
    expect(clean).not.toMatch(/position/);
  });

  it("removes 1x1 tracking pixels even with remote images on", () => {
    const dirty = `<p>Newsletter</p><img src="https://t.example.com/p.gif" width="1" height="1">`;
    const clean = sanitizeHtml(dirty, { allowRemoteImages: true });

    expect(clean).not.toMatch(/p\.gif/);
  });

  it("blocks protocol-relative and relative image sources, not inline ones", () => {
    const dirty =
      `<img src="//tracker.example/open.png" width="600">` +
      `<img src="images/logo.png" width="600">` +
      `<img src="cid:logo">` +
      `<img src="data:image/png;base64,AAAA">`;
    const clean = sanitizeHtml(dirty, { allowRemoteImages: false });

    expect(clean).not.toMatch(/tracker\.example|logo\.png/);
    expect(clean.match(/class="mxr-image-blocked"/g)).toHaveLength(2);
    expect(clean).toMatch(/src="cid:logo"/);
    expect(clean).toMatch(/src="data:image\/png/);
  });

  it("drops srcset, background attributes and style url() so nothing else loads", () => {
    const dirty =
      `<img src="data:image/png;base64,AAAA" srcset="https://cdn.example/a.png 2x, //cdn.example/b.png 3x">` +
      `<table background="https://cdn.example/bg.png"><tr><td style="background-image:url(//cdn.example/c.png);color:red">x</td></tr></table>` +
      `<div style="background: url('https://cdn.example/d.png')">y</div>`;
    const clean = sanitizeHtml(dirty, { allowRemoteImages: false });

    expect(clean).not.toMatch(/cdn\.example/);
    expect(clean).toMatch(/color: red/);
  });
});

describe("sanitizeReaderHtml", () => {
  const hostile = [
    `<p onclick="x()">hello</p>`,
    `<script>window.pwned = 1</script>`,
    `<img src="https://cdn.example/a.png" onerror="x()" alt="a">`,
    `<a href="javascript:alert(1)">js</a>`,
    `<a href="  JaVaScRiPt:alert(1)">js2</a>`,
    `<a href="data:text/html,<script>alert(1)</script>">data</a>`,
    `<style>body{display:none}</style>`,
    `<iframe src="https://evil.example"></iframe>`,
    `<form action="https://evil.example"><input name="q"><button>go</button></form>`,
    `<object data="x"></object><embed src="x">`,
    `<div style="position:fixed;inset:0;z-index:9999" class="fixed inset-0 z-50" id="root">overlay</div>`,
    `<svg onload="x()"><script>1</script></svg>`,
    `<base href="https://evil.example/"><link rel="stylesheet" href="https://evil.example/a.css">`,
  ].join("");

  it("leaves nothing active, restyling or app-relative in hostile markup", () => {
    const clean = sanitizeReaderHtml(hostile, { allowRemoteImages: true });
    const dom = new DOMParser().parseFromString(clean, "text/html");

    expect(
      dom.querySelectorAll(
        "script, style, iframe, object, embed, form, input, button, svg, base, link",
      ),
    ).toHaveLength(0);
    for (const element of dom.body.querySelectorAll("*")) {
      const names = element.getAttributeNames();
      expect(names.filter((name) => name.startsWith("on"))).toEqual([]);
      expect(names).not.toContain("style");
      expect(names).not.toContain("class");
      expect(names).not.toContain("id");
    }
    for (const anchor of dom.querySelectorAll("a")) {
      const href = anchor.getAttribute("href");
      expect(href === null || /^(https?:|mailto:)/i.test(href)).toBe(true);
    }
    expect(clean).not.toMatch(/javascript:/i);
    expect(clean).not.toMatch(/pwned|display:none/);
    expect(dom.body.textContent).toContain("hello");
  });

  it("opens links safely in a new tab", () => {
    const clean = sanitizeReaderHtml(
      `<a href="https://example.com/x" target="_self" rel="opener">x</a>`,
      {
        allowRemoteImages: false,
      },
    );
    const anchor = new DOMParser().parseFromString(clean, "text/html").querySelector("a");
    expect(anchor?.getAttribute("href")).toBe("https://example.com/x");
    expect(anchor?.getAttribute("target")).toBe("_blank");
    expect(anchor?.getAttribute("rel")).toBe("noopener noreferrer");
  });

  it("holds remote images back until allowed", () => {
    const html = `<p>before</p><img src="https://cdn.example/a.png" alt="chart"><p>after</p>`;
    const blocked = sanitizeReaderHtml(html, { allowRemoteImages: false });
    expect(blocked).not.toContain("cdn.example");
    expect(blocked).not.toContain("<img");
    expect(blocked).toContain("Image blocked");
    expect(blocked).toContain('class="mxr-image-blocked"');

    const allowed = sanitizeReaderHtml(html, { allowRemoteImages: true });
    expect(allowed).toContain(`src="https://cdn.example/a.png"`);
  });

  it("keeps the structure of a real newsletter", () => {
    const newsletter = `
      <h1>Issue 42</h1><h2>Top story</h2>
      <p>Text with <strong>bold</strong>, <em>emphasis</em> and a <a href="https://example.com/a">link</a>.</p>
      <figure><img src="data:image/png;base64,iVBORw0KGgo=" alt="x"><figcaption>Caption</figcaption></figure>
      <ul><li>one</li><li>two</li></ul><ol><li>first</li></ol>
      <blockquote>Quote</blockquote><pre><code>let x = 1;</code></pre>
      <table><thead><tr><th>A</th></tr></thead><tbody><tr><td colspan="2">cell</td></tr></tbody></table><hr>`;
    const clean = sanitizeReaderHtml(newsletter, { allowRemoteImages: false });
    const dom = new DOMParser().parseFromString(clean, "text/html");
    for (const selector of [
      "h1",
      "h2",
      "strong",
      "em",
      "a[href]",
      "figure img",
      "figcaption",
      "ul li",
      "ol li",
      "blockquote",
      "pre code",
      "table thead th",
      "td[colspan]",
      "hr",
    ]) {
      expect([selector, dom.querySelector(selector) !== null]).toEqual([selector, true]);
    }
  });
});
