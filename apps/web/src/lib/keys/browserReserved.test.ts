import { describe, expect, test } from "vitest";

import "@/lib/actions/catalog";
import { getRegistry } from "@/lib/actions/registry";

import { BROWSER_RESERVED, type Platform } from "./browserReserved";
import { parseChord } from "./chord";

const KNOWN_CONFLICTS: { id: string; chord: string; reason: string }[] = [
  {
    id: "compose.save",
    chord: "Mod+s",
    reason: "Save draft has always been ⌘S in compose; the composer claims it before Save Page",
  },
  {
    id: "compose.refresh",
    chord: "Mod+Shift+r",
    reason: "Refresh from daemon has always been ⇧⌘R in compose; it shadows hard reload there",
  },
];

function isKnownConflict(id: string, chord: string): boolean {
  return KNOWN_CONFLICTS.some((known) => known.id === id && known.chord === chord);
}

const ORDER = ["Cmd", "Ctrl", "Win", "Alt", "Shift"];

function physicalCombo(chord: string, platform: Platform): string {
  const tokens = parseChord(chord);
  return tokens
    .map((token) => {
      const parts = token.split("+");
      const key = (parts.at(-1) === "" ? "+" : (parts.at(-1) ?? "")).toLowerCase();
      const modifiers = parts.slice(0, parts.at(-1) === "" ? -2 : -1);
      const names = modifiers.map((modifier) => {
        if (modifier === "Mod") return platform === "mac" ? "Cmd" : "Ctrl";
        if (modifier === "Meta") return platform === "mac" ? "Cmd" : "Win";
        if (modifier === "Ctrl") return "Ctrl";
        return modifier;
      });
      const sorted = [...new Set(names)].toSorted((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
      return [...sorted, key].join("+");
    })
    .join(" ");
}

function reservedCombos(): { combo: string; platform: Platform; chord: string; reason: string }[] {
  return BROWSER_RESERVED.flatMap((entry) =>
    entry.platforms.map((platform) => ({
      combo: physicalCombo(entry.chord, platform),
      platform,
      chord: entry.chord,
      reason: entry.reason,
    })),
  );
}

function conflictsWith(chord: string, macOnly = false) {
  const hits: { platform: Platform; reserved: string; reason: string }[] = [];
  for (const platform of macOnly ? (["mac"] as const) : (["mac", "other"] as const)) {
    const combo = physicalCombo(chord, platform);
    for (const reserved of reservedCombos()) {
      if (reserved.platform === platform && reserved.combo === combo) {
        hits.push({ platform, reserved: reserved.chord, reason: reserved.reason });
      }
    }
  }
  return hits;
}

describe("browser-reserved shortcuts", () => {
  test("no registry binding uses a combo the browser keeps", () => {
    const problems: string[] = [];
    for (const binding of getRegistry().bindings()) {
      if (isKnownConflict(binding.action.id, binding.chord)) continue;
      const hits = conflictsWith(binding.chord, binding.macOnly);
      if (hits.length > 0) {
        problems.push(
          `${binding.action.id} "${binding.chord}" (${binding.scope}) is ${hits[0]?.reserved} on ${hits[0]?.platform}: ${hits[0]?.reason}`,
        );
      }
    }
    expect(problems).toEqual([]);
  });
});
