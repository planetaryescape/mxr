import { describe, expect, it } from "vitest";

import { analyzeBlockedContent, blockedSentence, sourceName } from "./blockedContent";

const NEWSLETTER = `
  <p>This week's issue</p>
  <img src="https://mcusercontent.com/hero.png" width="600">
  <img src="https://cdn.substackcdn.com/a.png">
  <img src="https://acme.list-manage.com/track/open.php?u=1" width="1" height="1">
  <img src="https://mailtrack.io/trace/mail/abc.png">
  <img src="cid:logo">
`;

describe("blocked content", () => {
  it("counts trackers apart from remote images and names who serves them", () => {
    expect(analyzeBlockedContent(NEWSLETTER)).toEqual({
      trackers: 2,
      remoteImages: 2,
      sources: ["Mailchimp", "Mailtrack", "Substack"],
    });
  });

  it("writes one factual sentence with correct plurals", () => {
    const content = analyzeBlockedContent(NEWSLETTER);
    expect(blockedSentence(content, false)).toBe(
      "Blocked 2 trackers and 2 remote images from Mailchimp, Mailtrack and 1 more.",
    );
    // Allowing images still leaves the trackers blocked.
    expect(blockedSentence(content, true)).toBe(
      "Blocked 2 trackers from Mailchimp, Mailtrack and 1 more.",
    );
    expect(blockedSentence({ trackers: 0, remoteImages: 1, sources: ["mxr.local"] }, false)).toBe(
      "Blocked 1 remote image from mxr.local.",
    );
    expect(blockedSentence({ trackers: 0, remoteImages: 3, sources: [] }, true)).toBeNull();
  });

  it("falls back to the registrable domain for unknown hosts", () => {
    expect(sourceName("images.news.example.com")).toBe("example.com");
    expect(sourceName("img.shop.example.co.uk")).toBe("example.co.uk");
    expect(sourceName("email.mg.acme.io")).toBe("Mailgun");
    expect(sourceName("demo.mxr.local")).toBe("mxr.local");
  });
});
