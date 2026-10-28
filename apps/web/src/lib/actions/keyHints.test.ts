import { afterEach, describe, expect, it } from "vitest";

import { formatChord } from "@/lib/keys/chord";
import { useKeymapStore } from "@/state/keymapStore";

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
const trash: Action = { ...archive, id: "mail.trash", label: "Move to Trash", shortcut: "#" };
const keyless: Action = { ...archive, id: "mail.keyless", shortcut: undefined };
const paletteOnly: Action = { ...archive, id: "mail.export", paletteOnly: true };

/** A daemon that marks each hint seen once, like `SetHintSeen`. */
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

  it("says nothing when another client already taught the key", async () => {
    const { nextTime, shown } = setup(["mail.archive"]);
    await nextTime.offer(archive);
    expect(shown).toEqual([]);
  });

  it("shows at most one every few minutes and offers the skipped one again", async () => {
    const { nextTime, shown, asked, advance } = setup();
    await nextTime.offer(archive);
    await nextTime.offer(trash);
    expect(asked).toEqual(["mail.archive"]);
    advance(HINT_GAP_MS);
    await nextTime.offer(trash);
    expect(shown.map((hint) => hint.text)).toEqual([
      "Archive. Next time: e",
      "Move to Trash. Next time: #",
    ]);
  });

  it("ignores actions without a key, and asks again after a failed write", async () => {
    const { nextTime, shown, asked, fail } = setup();
    await nextTime.offer(keyless);
    await nextTime.offer(paletteOnly);
    expect(asked).toEqual([]);
    fail(true);
    await nextTime.offer(archive);
    expect(shown).toEqual([]);
    fail(false);
    await nextTime.offer(archive);
    expect(shown.map((hint) => hint.actionId)).toEqual(["mail.archive"]);
  });

  it("uses the button's own words when the action's label is generic", async () => {
    const { nextTime, shown } = setup();
    const rowAction: Action = {
      ...archive,
      id: "list.row-action",
      label: "This list's row action",
      shortcut: "w",
    };
    await nextTime.offer(rowAction, "Done");
    expect(shown[0]?.text).toBe("Done. Next time: w");
  });
});

describe("Next time with single-key shortcuts off", () => {
  afterEach(() => {
    useKeymapStore.setState({ singleKeys: true });
  });

  it("names a key that still works, or none", async () => {
    useKeymapStore.setState({ singleKeys: false });
    const { nextTime, shown, advance } = setup();
    await nextTime.offer({ ...archive, aliases: ["Mod+e"] });
    expect(shown.map((hint) => hint.text)).toEqual([`Archive. Next time: ${formatChord("Mod+e")}`]);
    advance(HINT_GAP_MS);
    await nextTime.offer(trash);
    expect(shown).toHaveLength(1);
  });
});
