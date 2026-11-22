import { describe, expect, test } from "vitest";

import { elsewhereLinks } from "./deskLinks";

describe("everything-else links", () => {
  test("list only what has something, with week counts that open the places", () => {
    const links = elsewhereLinks({
      reading: 7,
      paper_trail: 3,
      deliveries: 0,
      invites: 1,
      screener: 2,
    });
    expect(links.map((link) => [link.label, link.count])).toEqual([
      ["Reading", 7],
      ["Updates", 3],
      ["Invites", 1],
      ["Screener", 2],
    ]);
    expect(links[0]).toMatchObject({ to: "/reading", suffix: "this week" });
    expect(links[1]).toMatchObject({ to: "/updates", suffix: "this week" });
  });

});
