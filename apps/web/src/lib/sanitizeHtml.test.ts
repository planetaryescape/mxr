import { describe, expect, it } from "vitest";

import { prepareEmailHtml, remoteHost, sanitizeHtml, sanitizeReaderHtml } from "./sanitizeHtml";

describe("sanitizeHtml", () => {
  it("strips inline <script> tags", () => {
    const dirty = `<p>hi</p><script>window.X=1</script>`;
    const clean = sanitizeHtml(dirty);

    expect(clean).not.toMatch(/<script/i);
    expect(clean).not.toMatch(/window\.X/);
  });

  it("keeps safe email presentation styles", () => {
    const dirty = `<p style="color:#777;font-size:12px;line-height:18px">hi</p>`;
    const clean = sanitizeHtml(dirty);

    expect(clean).toMatch(/style="color: #777; font-size: 12px; line-height: 18px"/);
  });

  it("keeps safe link hrefs and adds external-link attributes", () => {
    const dirty = `<a href="https://example.com/path">open</a>`;
    const clean = sanitizeHtml(dirty);

    expect(clean).toMatch(/href="https:\/\/example\.com\/path"/);
    expect(clean).toMatch(/target="_blank"/);
    expect(clean).toMatch(/rel="noopener noreferrer"/);
  });

  it("removes dangerous CSS from style attributes", () => {
    const dirty = `<p style="color:#777;background:url(http://evil/leak);position:fixed">hi</p>`;
    const clean = sanitizeHtml(dirty);

    expect(clean).toMatch(/style="color: #777"/);
    expect(clean).not.toMatch(/evil/);
    expect(clean).not.toMatch(/position/);
  });

  it("can strip light neutral backgrounds while preserving other safe styles", () => {
    const dirty = `<div style="background-color:#fff;color:#777;font-size:12px">hi</div>`;
    const clean = sanitizeHtml(dirty, { stripLightBackgrounds: true });

    expect(clean).toMatch(/style="color: #777; font-size: 12px"/);
    expect(clean).not.toMatch(/background-color/);
  });

  it("can strip dark text colors for dark email mode", () => {
    const dirty = `<p style="color:#172033;font-size:12px">hi</p>`;
    const clean = sanitizeHtml(dirty, { stripDarkTextColors: true });

    expect(clean).toMatch(/style="font-size: 12px"/);
    expect(clean).not.toMatch(/color/);
  });

  it("keeps non-neutral backgrounds when stripping light email containers", () => {
    const dirty = `<div style="background-color:#fde68a;color:#111">hi</div>`;
    const clean = sanitizeHtml(dirty, { stripLightBackgrounds: true });

    expect(clean).toMatch(/background-color: #fde68a/);
  });

  it("removes 1x1 tracking pixels even with remote images on", () => {
    const dirty = `<p>Newsletter</p><img src="https://t.example.com/p.gif" width="1" height="1">`;
    const clean = sanitizeHtml(dirty, { allowRemoteImages: true });

    expect(clean).not.toMatch(/p\.gif/);
  });

  it("keeps normal-sized product images", () => {
    const dirty = `<img src="https://cdn.example.com/hero.jpg" width="600" height="400" alt="hero">`;
    const clean = sanitizeHtml(dirty, { allowRemoteImages: true });

    expect(clean).toMatch(/hero\.jpg/);
    expect(clean).toMatch(/alt="hero"/);
  });

  it("keeps normal remote images by default", () => {
    const dirty = `<img src="https://cdn.example.com/hero.jpg" width="600" height="400" alt="hero">`;
    const clean = sanitizeHtml(dirty);

    expect(clean).toMatch(/src="https:\/\/cdn\.example\.com\/hero\.jpg"/);
  });

  it("removes known tracker-domain images regardless of size", () => {
    const dirty = `<img src="https://mailtrack.io/trace/abc" width="600" height="400">`;
    const clean = sanitizeHtml(dirty, { allowRemoteImages: true });

    expect(clean).not.toMatch(/mailtrack/);
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

  it("names the host of a protocol-relative source", () => {
    expect(remoteHost("//tracker.example/open.png")).toBe("tracker.example");
    expect(remoteHost("images/logo.png")).toBeNull();
    expect(remoteHost("cid:logo")).toBeNull();
  });

  it("adapts inherited dark text and a tinted callout together on the dark canvas", () => {
    const prepared = prepareEmailHtml(
      `<div style="color:#111"><div style="background:#e8f0fe">Important condition</div></div>`,
      { allowRemoteImages: false, preference: "dark", darkScheme: true },
    );

    expect(prepared.canvas).toBe("dark");
    expect(prepared.html).not.toMatch(/color:\s*#111/i);
    expect(prepared.html).not.toMatch(/background:\s*#e8f0fe/i);
    expect(prepared.html).toContain("Important condition");
  });

  it("keeps the authored foreground and tinted background paired on Auto's paper canvas", () => {
    const prepared = prepareEmailHtml(
      `<div style="color:#111"><div style="background:#e8f0fe">Important condition</div></div>`,
      { allowRemoteImages: false, preference: "auto", darkScheme: true },
    );

    expect(prepared.canvas).toBe("paper");
    expect(prepared.html).toMatch(/color:\s*#111/i);
    expect(prepared.html).toMatch(/background:\s*#e8f0fe/i);
  });

  it("uses an explicit Dark canvas even when the app scheme is light", () => {
    expect(
      prepareEmailHtml(`<p>message</p>`, {
        allowRemoteImages: false,
        preference: "dark",
        darkScheme: false,
      }).canvas,
    ).toBe("dark");
  });

  it("lets Auto follow the light app scheme while preserving sender colors", () => {
    const prepared = prepareEmailHtml(`<p style="color:#111">message</p>`, {
      allowRemoteImages: false,
      preference: "auto",
      darkScheme: false,
    });
    expect(prepared.canvas).toBe("original");
    expect(prepared.html).toMatch(/color:\s*#111/i);
  });

  it("adapts simple personal mail with neutral authored text in Auto", () => {
    const prepared = prepareEmailHtml(
      `<div style="color:rgb(34,34,34);font-family:Arial">Hi Alex, Thursday works.</div>`,
      { allowRemoteImages: false, preference: "auto", darkScheme: true },
    );

    expect(prepared.canvas).toBe("dark");
    expect(prepared.html).not.toMatch(/color:\s*rgb\(34,\s*34,\s*34\)/i);
    expect(prepared.html).toContain("Hi Alex, Thursday works.");
  });

  it("leaves a sender-authored white-on-gray pair untouched on Auto's paper canvas", () => {
    const prepared = prepareEmailHtml(`<div style="color:#fff;background:#777">Badge</div>`, {
      allowRemoteImages: false,
      preference: "auto",
      darkScheme: true,
    });

    expect(prepared.canvas).toBe("paper");
    expect(prepared.html).toMatch(/color:\s*#fff/i);
    expect(prepared.html).toMatch(/background:\s*#777/i);
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

  it("drops links that would navigate the app itself", () => {
    const clean = sanitizeReaderHtml(
      `<a href="/settings">a</a><a href="#top">b</a><a href="//evil.example/x">c</a><a href="mailto:a@b.co">d</a>`,
      { allowRemoteImages: true },
    );
    const hrefs = [
      ...new DOMParser().parseFromString(clean, "text/html").querySelectorAll("a"),
    ].map((anchor) => anchor.getAttribute("href"));
    expect(hrefs).toEqual([null, null, null, "mailto:a@b.co"]);
  });

  it("holds remote images back until allowed", () => {
    const html = `<p>before</p><img src="https://cdn.example/a.png" alt="chart"><p>after</p>`;
    const blocked = sanitizeReaderHtml(html, { allowRemoteImages: false });
    expect(blocked).not.toContain("cdn.example");
    expect(blocked).not.toContain("<img");
    expect(blocked).toContain("Image blocked");
    // The note keeps its own class (for styling) while the sender's classes go.
    expect(blocked).toContain('class="mxr-image-blocked"');

    const allowed = sanitizeReaderHtml(html, { allowRemoteImages: true });
    expect(allowed).toContain(`src="https://cdn.example/a.png"`);
  });

  it("always removes tracking pixels, even when remote images are allowed", () => {
    const clean = sanitizeReaderHtml(
      `<p>hi</p><img src="https://track.customer.io/e/open" alt=""><img src="https://cdn.example/p.gif" width="1" height="1">`,
      { allowRemoteImages: true },
    );
    expect(clean).not.toContain("<img");
  });

  it("keeps inline images and drops images the app would resolve against itself", () => {
    const clean = sanitizeReaderHtml(
      `<img src="data:image/png;base64,iVBORw0KGgo=" alt="inline"><img src="/api/v1/mail/threads"><img src="//cdn.example/a.png"><img src="cid:abc">`,
      { allowRemoteImages: true },
    );
    const dom = new DOMParser().parseFromString(clean, "text/html");
    expect([...dom.querySelectorAll("img")].map((img) => img.getAttribute("src"))).toEqual([
      "data:image/png;base64,iVBORw0KGgo=",
    ]);
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

  it("returns an empty string for empty input", () => {
    expect(sanitizeReaderHtml("", { allowRemoteImages: false })).toBe("");
  });
});
