import { describe, expect, test } from "vitest";

import {
  draftProvenanceLine,
  draftSourceLabel,
  draftSourceRoute,
  type DraftProvenance,
  type DraftSource,
} from "./draftProvenanceFormat";

function source(overrides: Partial<DraftSource> = {}): DraftSource {
  return {
    message_id: "m1",
    thread_id: "t1",
    date: "2026-09-12T10:00:00Z",
    from_me: true,
    person: "maya@example.com",
    person_name: "Maya Chen",
    ...overrides,
  };
}

function provenance(overrides: Partial<DraftProvenance> = {}): DraftProvenance {
  return {
    model: "gemma4",
    locality: "local",
    history_used: true,
    voice_examples: [],
    conversation: [],
    ...overrides,
  };
}

// The same wording as `DraftProvenanceData::summary_line` in mxr-protocol,
// so the CLI, TUI and web say one thing.
describe("draftProvenanceLine", () => {
  test("names the person when every example was to them", () => {
    const line = draftProvenanceLine(
      provenance({
        voice_examples: [source(), source({ message_id: "m2", person: "MAYA@example.com" })],
      }),
    );
    expect(line).toBe("Local model gemma4 · used 2 of your emails to Maya · history used");
  });

  test("mixed recipients and a rewrite are said plainly", () => {
    const line = draftProvenanceLine(
      provenance({
        voice_examples: [source(), source({ person: "sam@example.com", person_name: null })],
        rewrite: {
          model: "gpt-4o-mini",
          locality: "cloud",
          history_used: false,
          outcome: "applied",
        },
      }),
    );
    expect(line).toBe(
      "Local model gemma4 · used 2 of your emails · history used · rewritten by cloud model gpt-4o-mini",
    );
  });

  test("a rewrite that was not used, or not run, says so", () => {
    const rewrite = { model: "gpt-4o-mini", locality: "cloud", history_used: false } as const;
    expect(
      draftProvenanceLine(provenance({ rewrite: { ...rewrite, outcome: "rejected" } })),
    ).toMatch(/rewrite attempted by cloud model gpt-4o-mini, not used$/);
    expect(
      draftProvenanceLine(provenance({ rewrite: { ...rewrite, outcome: "skipped" } })),
    ).toMatch(/rewrite by cloud model gpt-4o-mini skipped to keep your history local$/);
  });

  test("a kept rewrite names a later dropped pass on its own", () => {
    expect(
      draftProvenanceLine(
        provenance({
          rewrite: {
            model: "first-pass",
            locality: "local",
            history_used: true,
            outcome: "applied",
            rejected_by: "second-pass",
          },
        }),
      ),
    ).toMatch(/rewritten by local model first-pass, a later pass by second-pass not used$/);
  });

  test("a cloud model that saw no history says so", () => {
    expect(draftProvenanceLine(provenance({ locality: "cloud", history_used: false }))).toBe(
      "Cloud model gemma4 · history not used",
    );
  });
});

describe("draft sources", () => {
  test("your own email opens from Sent, anyone else's from All Mail, on that message", () => {
    expect(draftSourceRoute(source())).toEqual({
      to: "/m/$mailbox/$threadId",
      params: { mailbox: "sent", threadId: "t1" },
      search: { message: "m1" },
    });
    expect(draftSourceRoute(source({ from_me: false })).params.mailbox).toBe("archive");
  });

  test("labels say whose message it is", () => {
    expect(draftSourceLabel(source(), "voice")).toBe("Your email to Maya");
    expect(draftSourceLabel(source({ from_me: false }), "conversation")).toBe("Maya");
    expect(draftSourceLabel(source(), "conversation")).toBe("You");
    expect(draftSourceLabel(source({ from_me: false, person_name: null }), "conversation")).toBe(
      "maya@example.com",
    );
  });
});
