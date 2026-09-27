import { describe, expect, test } from "vitest";

import { inviteReplyIntent } from "./composeUiStore";

describe("inviteReplyIntent", () => {
  test("names the response and keys each action separately", () => {
    const decline = inviteReplyIntent("m-1", "decline");
    expect(decline).toEqual({
      key: "compose:invite_reply:decline:m-1",
      title: "Decline with comment",
      kind: "invite_reply",
      messageId: "m-1",
      inviteAction: "decline",
    });
    expect(inviteReplyIntent("m-1", "accept").key).not.toBe(decline.key);
  });
});
