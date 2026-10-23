import { describe, expect, test } from "vitest";

import { replyWithBodyIntent } from "../composeUiStore";
import {
  addressList,
  applyPrefill,
  draftFromSession,
  type ComposeDraftState,
} from "./composeDraft";

function draft(bodyMarkdown: string): ComposeDraftState {
  return {
    draftPath: "/tmp/d.md",
    rawContent: "",
    frontmatter: {
      to: "alice@x.com",
      cc: "",
      bcc: "",
      subject: "Re: Friday",
      from: "",
      attach: [],
    },
    bodyMarkdown,
    issues: [],
    accountId: "acc",
    kind: "reply",
  };
}

describe("applyPrefill", () => {
  test("an AI draft goes above the quoted history of a reply", () => {
    const intent = replyWithBodyIntent("m-1", "yep, friday works");
    const { draft: next, changed } = applyPrefill(draft("> Can you do Friday?"), intent);
    expect(changed).toBe(true);
    expect(next.bodyMarkdown).toBe("yep, friday works\n\n> Can you do Friday?");
    // Resuming the same session does not insert it twice.
    expect(applyPrefill(next, intent).changed).toBe(false);
  });

  test("each AI reply gets its own session", () => {
    expect(replyWithBodyIntent("m-1", "a").key).toMatch(/^compose:reply:m-1:draft-/);
  });
});

test("session hydration keeps the saved reply or forward intent beside editable kind", () => {
  const savedSession = {
    draftPath: "/tmp/saved-forward.md",
    rawContent: "",
    bodyMarkdown: "Forward note",
    frontmatter: {
      to: "pat@example.com",
      cc: "",
      bcc: "",
      subject: "Forwarded subject",
      from: "me@example.com",
      attach: [],
    },
    previewHtml: "",
    issues: [],
    kind: "new",
    draftId: "saved-forward",
    revision: 2,
    draftIntent: "forward" as const,
  };
  const restoredDraft = draftFromSession(savedSession);
  expect(restoredDraft.kind).toBe("new");
  expect(restoredDraft.draftIntent).toBe("forward");
});

describe("addressList", () => {
  test("splits names and bare addresses into the daemon's shape", () => {
    expect(addressList("Pat Lee <pat@example.com>, sam@example.com")).toEqual([
      { name: "Pat Lee", email: "pat@example.com" },
      { name: null, email: "sam@example.com" },
    ]);
  });

  test("an empty recipient field has no addresses", () => {
    expect(addressList("")).toEqual([]);
  });
});
