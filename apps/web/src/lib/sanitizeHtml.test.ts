import { describe, expect, it } from "vitest";

import { remoteHost, sanitizeHtml } from "./sanitizeHtml";

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
});
