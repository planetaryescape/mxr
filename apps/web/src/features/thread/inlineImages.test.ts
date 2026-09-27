/* @vitest-environment jsdom */

import { describe, expect, test } from "vitest";

import { inlineImageSources, replaceImageSources } from "./inlineImages";

describe("inline images", () => {
  test("finds each cid image once", () => {
    const html = `<img src="cid:a@x"><img src="cid:a@x"><img src="https://x.test/b.png">`;
    expect(inlineImageSources(html)).toEqual(["cid:a@x"]);
  });

  test("loaded parts become data URIs; missing ones drop src and keep alt", () => {
    const html = `<img src="cid:a@x" alt="Logo"><img src="cid:gone@x" alt="Chart"><img src="https://x.test/c.png">`;
    const out = replaceImageSources(html, new Map([["cid:a@x", "data:image/png;base64,AA=="]]));
    const doc = new DOMParser().parseFromString(out, "text/html");
    const [logo, chart, remote] = Array.from(doc.querySelectorAll("img"));
    expect(logo?.getAttribute("src")).toBe("data:image/png;base64,AA==");
    expect(chart?.hasAttribute("src")).toBe(false);
    expect(chart?.getAttribute("alt")).toBe("Chart");
    // Remote images are the sanitizer's and the remote-image gate's business.
    expect(remote?.getAttribute("src")).toBe("https://x.test/c.png");
  });
});
