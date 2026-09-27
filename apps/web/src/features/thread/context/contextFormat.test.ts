import { describe, expect, it } from "vitest";

import type { ThreadContext, ThreadCounterparty } from "./api";
import {
  dayLabel,
  owedReplyLabel,
  promiseViews,
  provenanceLabel,
  relationshipParts,
  shortDuration,
} from "./contextFormat";

const NOW = new Date(2026, 8, 27, 12, 0);

function person(overrides: Partial<ThreadCounterparty> = {}): ThreadCounterparty {
  return {
    email: "maya@example.com",
    display_name: "Maya Ortiz",
    messages_from_them: 23,
    messages_from_you: 18,
    your_reply_p50_seconds: 4 * 3600 - 60,
    your_reply_samples: 12,
    their_reply_p50_seconds: null,
    their_reply_samples: 0,
    last_contact_elsewhere_at: new Date(2026, 8, 12, 9).toISOString(),
    bulk_sender: false,
    ...overrides,
  };
}

function context(overrides: Partial<ThreadContext> = {}): ThreadContext {
  return {
    thread_id: "t1",
    account_id: "a1",
    counterparty: person(),
    commitments: [],
    ...overrides,
  };
}

describe("context copy", () => {
  it("says how you know the person in one line", () => {
    expect(relationshipParts(person(), NOW)).toEqual([
      "You and Maya: 41 emails",
      "you usually reply within 4h",
      `last spoke ${dayLabel(new Date(2026, 8, 12, 9).toISOString(), NOW)}`,
    ]);
    expect(
      relationshipParts(
        person({
          messages_from_them: 1,
          messages_from_you: 0,
          your_reply_p50_seconds: null,
          last_contact_elsewhere_at: null,
        }),
        NOW,
      ),
    ).toEqual(["You and Maya: 1 email", "your first conversation"]);
    expect(relationshipParts(person({ bulk_sender: true, display_name: null }), NOW)).toEqual([
      "Bulk mail from maya@example.com: 23 emails",
    ]);
  });

  it("uses relative days when recent and dates when older", () => {
    expect(dayLabel(new Date(2026, 8, 27, 8).toISOString(), NOW)).toBe("today");
    expect(dayLabel(new Date(2026, 8, 26, 8).toISOString(), NOW)).toBe("yesterday");
    expect(dayLabel(new Date(2025, 0, 3).toISOString(), NOW)).toMatch(/2025/);
    expect(shortDuration(30)).toBe("1m");
    expect(shortDuration(3 * 86_400)).toBe("3d");
  });

  it("names promises both ways and the owed reply", () => {
    const facts = context({
      owed_reply: { message_id: "m1", since: new Date(2026, 8, 26, 8).toISOString() },
      commitments: [
        {
          id: "c1",
          account_id: "a1",
          email: "maya@example.com",
          thread_id: "t1",
          direction: "yours",
          status: "open",
          who_owes: "you",
          what: "send the runbook link",
          by_when: new Date(2026, 8, 27, 17).toISOString(),
          evidence_msg_id: "m1",
          extracted_at: NOW.toISOString(),
        },
        {
          id: "c2",
          account_id: "a1",
          email: "maya@example.com",
          thread_id: "t1",
          direction: "theirs",
          status: "open",
          who_owes: "maya",
          what: "share the dashboard",
          evidence_msg_id: "m1",
          extracted_at: NOW.toISOString(),
        },
      ],
    });
    expect(owedReplyLabel(facts, NOW)).toBe("you owe a reply since yesterday");
    expect(promiseViews(facts, NOW).map((view) => view.text)).toEqual([
      "You promised: send the runbook link, due today",
      "Maya promised: share the dashboard",
    ]);
  });

  it("says which model wrote the gist and what it read", () => {
    expect(
      provenanceLabel({ model: "qwen2.5", locality: "local", sources: ["this_thread"] }, "Maya"),
    ).toBe("Local model qwen2.5 · from this thread");
    expect(
      provenanceLabel(
        { model: "gpt-4o-mini", locality: "cloud", sources: ["this_thread"] },
        "Maya",
      ),
    ).toBe("Cloud model gpt-4o-mini · from this thread only");
    expect(
      provenanceLabel(
        { model: "qwen2.5", locality: "local", sources: ["this_thread", "relationship_history"] },
        "Maya",
      ),
    ).toBe("Local model qwen2.5 · from this thread and your history with Maya");
  });

  it("never uses em dashes", () => {
    const all = [
      ...relationshipParts(person(), NOW),
      provenanceLabel({ model: "m", locality: "cloud", sources: ["this_thread"] }, null),
    ].join(" ");
    expect(all).not.toContain("—");
  });
});
