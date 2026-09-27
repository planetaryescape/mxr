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

  test("the screener link opens the account whose senders were counted", () => {
    const [screener] = elsewhereLinks({
      reading: 0,
      paper_trail: 0,
      deliveries: 0,
      invites: 0,
      screener: 4,
      screener_account: "acct-2",
    });
    expect(screener).toMatchObject({ to: "/screener", search: { account: "acct-2" } });
  });
});
