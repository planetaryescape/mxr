/* @vitest-environment node */

/*
 * The web registry against mxr's one key definition (`crates/keymap/keymap.toml`,
 * blueprint chapter 35). `keymap.generated.json` is written from it by
 * `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`. Until the registry reads its keys
 * from that file, a key changed here must be changed there too.
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
}

const defined = (keymap.action as DefinedAction[]).filter(
  (action) => !action.clients || action.clients.includes("web"),
);

function definedBindings(): string[] {
  return defined.flatMap((action) =>
    (action.scopes ?? ["global"]).flatMap((scope) => [
      ...(action.keys ?? []).map((chord) => `${scope}|${chord}|${action.id}`),
      ...(action.mac_keys ?? []).map((chord) => `${scope}|${chord}|${action.id}|mac`),
      ...(action.retired_keys ?? []).map((chord) => `${scope}|${chord}|${action.id}|retired`),
    ]),
  );
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
        : [`${want.id}: registry ${JSON.stringify(have)} ≠ keymap.toml ${JSON.stringify(expected)}`];
    });
    expect(mismatched).toEqual([]);
  });

  test("bind the same keys", () => {
    const registered = getRegistry()
      .bindings()
      .map(
        (b) =>
          `${b.scope}|${b.chord}|${b.action.id}${b.macOnly ? "|mac" : ""}${b.retired ? "|retired" : ""}`,
      )
      .toSorted();
    expect(registered).toEqual(definedBindings().toSorted());
  });
});
