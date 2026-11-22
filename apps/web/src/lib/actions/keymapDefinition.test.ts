/* @vitest-environment node */

import { describe, expect, test } from "vitest";

import "./catalog";
import { isOverlayKey, MXR_VERSION, withoutExpiredMoves, type DefinedAction } from "./definition";
import keymap from "./keymap.generated.json";
import { getRegistry } from "./registry";

const defined = withoutExpiredMoves(keymap.action as DefinedAction[], MXR_VERSION).filter(
  (action) => (!action.clients || action.clients.includes("web")) && !isOverlayKey(action),
);

const TUI_ONLY_CONTEXTS = new Set([
  "place",
  "rules",
  "accounts",
  "invites",
  "not-sure",
  "arrivals-list",
  "desk",
]);

function definedBindings(): string[] {
  const bindings: string[] = [];
  for (const action of defined) {
    for (const scope of action.scopes ?? ["global"]) {
      for (const chord of action.keys ?? []) bindings.push(`${scope}|${chord}|${action.id}`);
      for (const chord of action.mac_keys ?? []) {
        bindings.push(`${scope}|${chord}|${action.id}|mac`);
      }
      for (const chord of action.retired_keys ?? []) {
        bindings.push(`${scope}|${chord}|${action.id}|retired`);
      }
    }
    for (const { key, contexts } of action.moved ?? []) {
      for (const scope of contexts.filter((context) => !TUI_ONLY_CONTEXTS.has(context))) {
        bindings.push(`${scope}|${key}|${action.id}|moved`);
      }
    }
  }
  return bindings;
}

describe("the web registry and the key definition", () => {
  test("define the same actions", () => {
    const registered = getRegistry()
      .all()
      .map((action) => action.id)
      .toSorted();
    expect(registered).toEqual(defined.map((action) => action.id).toSorted());
  });

  test("describe each action the same way", () => {
    const mismatched = defined.flatMap((want) => {
      const action = getRegistry().get(want.id);
      if (!action) return [];
      const have = {
        label: action.label,
        short_label: action.shortLabel,
        description: action.description,
        group: action.group,
        scopes: action.scopes ?? ["global"],
        keys: [action.shortcut, ...(action.aliases ?? [])].filter(Boolean),
        mac_keys: action.macAliases ?? [],
        retired_keys: action.retiredAliases ?? [],
        palette_only: !!action.paletteOnly,
        hide_in_palette: !!action.hideInPalette,
        note: action.tuiNote,
      };
      const expected = {
        label: want.label,
        short_label: want.short_label,
        description: want.description,
        group: want.group,
        scopes: want.scopes ?? ["global"],
        keys: want.keys ?? [],
        mac_keys: want.mac_keys ?? [],
        retired_keys: want.retired_keys ?? [],
        palette_only: !!want.palette_only,
        hide_in_palette: !!want.hide_in_palette,
        note: want.note,
      };
      return JSON.stringify(have) === JSON.stringify(expected)
        ? []
        : [
            `${want.id}: registry ${JSON.stringify(have)} ≠ keymap.toml ${JSON.stringify(expected)}`,
          ];
    });
    expect(mismatched).toEqual([]);
  });

  test("bind the same keys", () => {
    const registered = getRegistry()
      .bindings()
      .map(
        (b) =>
          `${b.scope}|${b.chord}|${b.action.id}${b.macOnly ? "|mac" : ""}${b.moved ? "|moved" : b.retired ? "|retired" : ""}`,
      )
      .toSorted();
    expect(registered).toEqual(definedBindings().toSorted());
  });
});
