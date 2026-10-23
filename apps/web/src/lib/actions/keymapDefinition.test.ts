/* @vitest-environment node */

/*
 * The web registry against mxr's one key definition (`crates/keymap/keymap.toml`,
 * blueprint chapter 35). `keymap.generated.json` is written from it by
 * `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`, and the registry is built from
 * that file (`fromDefinition`), so this guards the merge: every web action in
 * the definition has a behaviour, and nothing is lost or changed on the way.
 */

import { describe, expect, test } from "vitest";

import "./catalog";
import keymap from "./keymap.generated.json";
import { getRegistry } from "./registry";

interface DefinedAction {
  id: string;
  label: string;
  short_label?: string;
  description?: string;
  group: string;
  scopes?: string[];
  keys?: string[];
  mac_keys?: string[];
  retired_keys?: string[];
  palette_only?: boolean;
  hide_in_palette?: boolean;
  note?: string;
  clients?: string[];
  moved?: { key: string; contexts: string[] }[];
}

const defined = (keymap.action as DefinedAction[]).filter(
  (action) => !action.clients || action.clients.includes("web"),
);

// The TUI's place view and the screens that still read their own keys have
// no web scope.
const TUI_ONLY_CONTEXTS = new Set(["place", "rules", "accounts", "invites"]);

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
      // A move lists TUI contexts too; the TUI's place view has no web scope.
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
