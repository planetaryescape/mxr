import { describe, expect, it } from "vitest";

import { createNextTime, HINT_GAP_MS, type KeyHint } from "./keyHints";
import type { Action } from "./types";

const archive: Action = {
  id: "mail.archive",
  command: "archive",
  label: "Archive",
  shortcut: "e",
  group: "Mail",
  scopes: ["list", "reader"],
};

function setup(seenElsewhere: string[] = []) {
  let time = 1_000_000;
  const seen = new Set(seenElsewhere);
  const asked: string[] = [];
  const shown: KeyHint[] = [];
  let failing = false;
  const nextTime = createNextTime({
    ask: async (id) => {
      asked.push(id);
      if (failing) throw new Error("daemon down");
      if (seen.has(id)) return false;
      seen.add(id);
      return true;
    },
    show: (hint) => shown.push(hint),
    now: () => time,
  });
  return {
    nextTime,
    asked,
    shown,
    advance: (ms: number) => {
      time += ms;
    },
    fail: (value: boolean) => {
      failing = value;
    },
  };
}

describe("Next time", () => {
  it("names the key on the first palette or pointer run, once per profile", async () => {
    const { nextTime, shown, asked, advance } = setup();
    await nextTime.offer(archive);
    expect(shown.map((hint) => hint.text)).toEqual(["Archive. Next time: e"]);
    advance(HINT_GAP_MS * 10);
    await nextTime.offer(archive);
    await nextTime.offer(archive);
    expect(shown).toHaveLength(1);
    expect(asked).toEqual(["mail.archive"]);
  });
});
