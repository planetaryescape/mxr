import { describe, expect, it } from "vitest";

import { analyzeBlockedContent, blockedSentence, sourceName } from "./blockedContent";

const NEWSLETTER = `
  <p>This week's issue</p>
  <img src="https://mcusercontent.com/hero.png" width="600">
  <img src="https://cdn.substackcdn.com/a.png">
  <img src="https://acme.list-manage.com/track/open.php?u=1" width="1" height="1">
  <img src="https://mailtrack.io/trace/mail/abc.png">
  <img src="cid:logo">
  <img src="//pixel.example.net/open" width="1" height="1">
  <img src="//images.example.org/banner.png" width="600">
`;

describe("blocked content", () => {
  it("counts trackers apart from remote images and names who serves them", () => {
    expect(analyzeBlockedContent(NEWSLETTER)).toEqual({
      trackers: 3,
      remoteImages: 3,
      trackerSources: ["Mailchimp", "Mailtrack", "example.net"],
      imageSources: ["Mailchimp", "Substack", "example.org"],
    });
  });

  it("writes one factual sentence with correct plurals", () => {
    const content = analyzeBlockedContent(NEWSLETTER);
    expect(blockedSentence(content, false)).toBe(
      "Blocked 3 trackers and 3 remote images from Mailchimp, Mailtrack and 3 more.",
    );
    // Allowing images leaves the trackers blocked, and names only their sources.
    expect(blockedSentence(content, true)).toBe(
      "Blocked 3 trackers from Mailchimp, Mailtrack and 1 more.",
    );
    const imagesOnly = {
      trackers: 0,
      remoteImages: 1,
      trackerSources: [],
      imageSources: ["mxr.local"],
    };
    expect(blockedSentence(imagesOnly, false)).toBe("Blocked 1 remote image from mxr.local.");
    expect(blockedSentence(imagesOnly, true)).toBeNull();
    const mixed = {
      trackers: 1,
      remoteImages: 2,
      trackerSources: ["Mailtrack"],
      imageSources: ["Substack"],
    };
    expect(blockedSentence(mixed, true)).toBe("Blocked 1 tracker from Mailtrack.");
  });

  it("falls back to the registrable domain for unknown hosts", () => {
    expect(sourceName("images.news.example.com")).toBe("example.com");
    expect(sourceName("img.shop.example.co.uk")).toBe("example.co.uk");
    expect(sourceName("email.mg.acme.io")).toBe("Mailgun");
    expect(sourceName("demo.mxr.local")).toBe("mxr.local");
  });
});
