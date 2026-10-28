import { describe, expect, test } from "vitest";

import "@/lib/actions/catalog";
import keymap from "@/lib/actions/keymap.generated.json";
import { getRegistry } from "@/lib/actions/registry";

import { BROWSER_RESERVED, type Platform } from "./browserReserved";
import { parseChord } from "./chord";

/**
 * Bindings a page declares outside the registry. Each is a literal in the
 * source named here; the test checks it against the reserved list, so a
 * new literal is caught only once it is added here.
 */
const LITERAL_BINDINGS: { chord: string; source: string }[] = [
  { chord: "Ctrl+j", source: "features/search/SearchPalette.tsx:90" },
  { chord: "Ctrl+k", source: "features/search/SearchPalette.tsx:94" },
];

/**
 * Registry bindings that do use a reserved combo, kept because this rung of
 * the keyboard plan changes no key (blueprint 35, K1). Each moves when the
 * key map does (K3), and the list must then be empty.
 */
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

/**
 * The physical key combo a chord asks for on one platform, so `Mod` and
 * `Meta` mean what the browser will read them as.
 */
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

  test("each known conflict is a registry binding that still conflicts", () => {
    const bindings = getRegistry().bindings();
    const stale = KNOWN_CONFLICTS.filter(
      (known) =>
        !bindings.some((b) => b.action.id === known.id && b.chord === known.chord) ||
        conflictsWith(known.chord).length === 0,
    );
    expect(stale).toEqual([]);
  });

  test("every compose key in the definition is a registry binding the check above sees", () => {
    const bound = new Set(
      getRegistry()
        .bindings()
        .filter((binding) => binding.scope === "compose")
        .map((binding) => `${binding.action.id} ${binding.chord}`),
    );
    const defined = (keymap.action as { id: string; scopes?: string[]; keys?: string[] }[])
      .filter((action) => action.scopes?.includes("compose"))
      .flatMap((action) => (action.keys ?? []).map((chord) => `${action.id} ${chord}`));
    expect(defined.length).toBeGreaterThan(0);
    expect(defined.filter((entry) => !bound.has(entry))).toEqual([]);
  });

  test("no literal binding in dialogs uses a reserved combo", () => {
    const problems: string[] = [];
    for (const literal of LITERAL_BINDINGS) {
      const hits = conflictsWith(literal.chord);
      if (hits.length > 0) {
        problems.push(`${literal.source} "${literal.chord}" is ${hits[0]?.reserved}`);
      }
    }
    expect(problems).toEqual([]);
  });

  test("macOS-only aliases are inert off macOS", () => {
    const macOnly = getRegistry()
      .bindings()
      .filter((binding) => binding.macOnly);
    expect(macOnly.map((binding) => `${binding.scope} ${binding.chord}`).toSorted()).toEqual([
      "global Ctrl+p",
      "list Ctrl+f",
      "messages Ctrl+f",
    ]);
    for (const binding of macOnly) {
      expect(getRegistry().resolve(binding.chord, [binding.scope], false)).toBeUndefined();
    }
    // The filters need a mounted controller, so only the palette's alias
    // can be resolved here.
    expect(getRegistry().resolve("Ctrl+p", ["global"], true)?.action.id).toBe(
      "shell.command-palette",
    );
  });

  test("the chord normaliser agrees with the browser on the combos that matter", () => {
    expect(physicalCombo("Mod+Shift+t", "mac")).toBe("Cmd+Shift+t");
    expect(physicalCombo("Mod+Shift+t", "other")).toBe("Ctrl+Shift+t");
    expect(physicalCombo("Meta+h", "mac")).toBe("Cmd+h");
    expect(physicalCombo("Mod+Enter", "mac")).toBe("Cmd+enter");
  });
});
