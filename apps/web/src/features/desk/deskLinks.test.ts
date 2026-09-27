import { describe, expect, test } from "vitest";

import { elsewhereLinks } from "./deskLinks";

describe("everything-else links", () => {
  test("list only what has something, and send paper trail to the inbox", () => {
    const links = elsewhereLinks({
      reading: 7,
      paper_trail: 3,
      deliveries: 0,
      invites: 1,
      screener: 2,
    });
    expect(links.map((link) => [link.label, link.count])).toEqual([
      ["Reading", 7],
      ["Paper trail", 3],
      ["Invites", 1],
      ["Screener", 2],
    ]);
    expect(links[1]).toMatchObject({ to: "/m/$mailbox", params: { mailbox: "inbox" } });
  });
});
