/*
 * mxr's one key definition (`crates/keymap/keymap.toml`, blueprint chapter
 * 35), as the web reads it. `keymap.generated.json` is written from the TOML
 * by `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`; every action's label, keys
 * and scopes come from there, so the TUI, `mxr keys` and the web can't
 * disagree. Feature modules declare only what an action does.
 */

import keymap from "./keymap.generated.json";
import type { Action, ActionBehavior, ActionGroup, ActionScope } from "./types";

export interface DefinedAction {
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
  moved?: { key: string; contexts: string[]; until: string; to?: string; to_key?: string }[];
  web_native?: boolean;
}

// Records rather than arrays so the compiler fails when a group or scope is
// added to the union but not here. The Rust side checks scopes but not
// groups, so a misspelt group would otherwise reach help as a new section.
const GROUPS: Record<ActionGroup, true> = {
  Mail: true,
  Compose: true,
  Search: true,
  Semantic: true,
  Accounts: true,
  Navigate: true,
  Settings: true,
  Diagnostics: true,
  Rules: true,
  Analytics: true,
  Triage: true,
  View: true,
  Move: true,
  Select: true,
  Read: true,
};

const SCOPES: Record<ActionScope, true> = {
  global: true,
  sidebar: true,
  list: true,
  reader: true,
  screener: true,
  focus: true,
  todo: true,
  catchup: true,
  expired: true,
  now: true,
  messages: true,
  "messages-filters": true,
  page: true,
  updates: true,
  archive: true,
  reading: true,
  "reading-reader": true,
  compose: true,
  arrivals: true,
};

/** The definition's web actions by id. Actions without "web" in `clients` are the TUI's alone. */
const definedActions = new Map(
  (keymap.action as DefinedAction[])
    .filter((action) => !action.clients || action.clients.includes("web"))
    .map((action) => [action.id, action]),
);

/** The definition with the person's keys, once the daemon has served them. */
let webActions = definedActions;

/** What the daemon serves (`GetKeymap`): the key map with `keys.toml` applied. */
export interface ServedKeymap {
  actions: { id: string; keys?: string[]; mac_keys?: string[] }[];
  single_keys?: boolean;
  remapped?: string[];
  problems?: string[];
}

/**
 * The definition with the daemon's keys for each action the person
 * remapped. The generated JSON stays the source of everything else (labels,
 * moved keys, retired aliases), so an older daemon can't blank the app. A
 * moved-key notice on a key the person now uses goes, as the daemon drops it.
 */
export function withServedKeys(
  defined: ReadonlyMap<string, DefinedAction>,
  served: ServedKeymap,
): Map<string, DefinedAction> {
  const result = new Map(defined);
  const servedById = new Map(served.actions.map((action) => [action.id, action]));
  for (const id of served.remapped ?? []) {
    const own = result.get(id);
    const keys = servedById.get(id)?.keys;
    if (!own || !keys) continue;
    result.set(id, { ...own, keys, mac_keys: [], retired_keys: [] });
  }
  if (!served.remapped?.length) return result;
  const bound: { scope: string; chord: string }[] = [];
  for (const action of result.values()) {
    for (const scope of action.scopes?.length ? action.scopes : ["global"]) {
      for (const chord of action.keys ?? []) bound.push({ scope, chord });
    }
  }
  const shadowed = (scope: string, key: string) =>
    bound.some(
      ({ scope: where, chord }) =>
        (where === scope || where === "global") &&
        (chord === key || chord.startsWith(`${key} `) || key.startsWith(`${chord} `)),
    );
  for (const [id, action] of result) {
    if (!action.moved?.length) continue;
    const moved = action.moved
      .map((entry) => ({
        ...entry,
        contexts: entry.contexts.filter((context) => !shadowed(context, entry.key)),
      }))
      .filter((entry) => entry.contexts.length > 0);
    result.set(id, { ...action, moved });
  }
  return result;
}

/**
 * An overlay's key (blueprint 35, "Overlays share one grammar"): every scope
 * it names is a dialog, which reads it with `definedKeys` while it owns the
 * keyboard. It is no dispatcher binding, so the registry never holds it.
 */
export function isOverlayKey(action: Pick<DefinedAction, "scopes">): boolean {
  const scopes = action.scopes ?? [];
  return scopes.length > 0 && scopes.every((scope) => !isScope(scope));
}

/** An overlay action's keys on this platform: its keys, and on macOS its mac-only ones too. */
export function definedOverlayKeys(id: string, mac: boolean): string[] {
  const action = webActions.get(id);
  if (!action?.keys?.length) {
    throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  }
  return mac ? [...action.keys, ...(action.mac_keys ?? [])] : action.keys;
}

/** The action whose key `chord` moved away from in overlay `place`, while the move is announced. */
export function movedOverlayKey(place: string, chord: string): DefinedAction | undefined {
  for (const action of webActions.values()) {
    if (action.moved?.some((move) => move.key === chord && move.contexts.includes(place))) {
      return action;
    }
  }
  return undefined;
}

/** Use the daemon's keys from now on; `null` goes back to the definition's. */
export function setServedKeys(served: ServedKeymap | null): void {
  webActions = served ? withServedKeys(definedActions, served) : definedActions;
}

function isGroup(group: string): group is ActionGroup {
  return Object.hasOwn(GROUPS, group);
}

function isScope(scope: string): scope is ActionScope {
  return Object.hasOwn(SCOPES, scope);
}

/** The full action: `behavior` plus everything the definition says about its id. */
export function fromDefinition(behavior: ActionBehavior): Action {
  const { id } = behavior;
  const defined = webActions.get(id);
  if (!defined) {
    throw new Error(
      `Action "${id}" is not a web action in crates/keymap/keymap.toml; add it there and run \`UPDATE_KEYMAP=1 cargo test -p mxr-keymap\``,
    );
  }
  const { group } = defined;
  if (!isGroup(group)) {
    throw new Error(`Action "${id}" has unknown group "${group}" in crates/keymap/keymap.toml`);
  }
  const scopes = defined.scopes?.map((scope) => {
    if (!isScope(scope)) {
      throw new Error(`Action "${id}" has unknown scope "${scope}" in crates/keymap/keymap.toml`);
    }
    return scope;
  });
  const [shortcut, ...aliases] = defined.keys ?? [];
  return {
    ...behavior,
    label: defined.label,
    shortLabel: defined.short_label,
    description: defined.description,
    group,
    scopes,
    shortcut,
    aliases: aliases.length > 0 ? aliases : undefined,
    macAliases: defined.mac_keys,
    retiredAliases: defined.retired_keys,
    movedAliases: defined.moved?.map(({ key, contexts, to, to_key }) => ({
      chord: key,
      // TUI-only places in the list ("place") are not web scopes.
      scopes: contexts.filter(isScope),
      // The key to name when the action has none left (digit 1 → g i), or
      // a key on a screen that still reads its own keys.
      to: to_key ?? (to === undefined ? undefined : webActions.get(to)?.keys?.[0]),
    })),
    paletteOnly: defined.palette_only,
    hideInPalette: defined.hide_in_palette,
    tuiNote: defined.note,
  };
}

/** An action's primary key, for UI outside the registry that names it. */
export function definedShortcut(id: string): string {
  const shortcut = webActions.get(id)?.keys?.[0];
  if (!shortcut) throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  return shortcut;
}

/** Every web key an action has, primary first, for handlers outside the registry. */
export function definedKeys(id: string): string[] {
  const keys = webActions.get(id)?.keys;
  if (!keys?.length) throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  return keys;
}

const strips = new Map(
  ((keymap as { strip?: { id: string; actions: string[] }[] }).strip ?? []).map((strip) => [
    strip.id,
    strip.actions,
  ]),
);

/**
 * The status bar's verbs for a state (`[[strip]]` in the definition), minus
 * any that don't apply to the lit item. The TUI's hint bar reads the same
 * strips, so one state shows the same verbs in both clients.
 */
export function stripKeys(id: string, omit: readonly string[] = []): string[] {
  const actions = strips.get(id);
  if (!actions) throw new Error(`No strip "${id}" in crates/keymap/keymap.toml`);
  return actions.filter((action) => !omit.includes(action));
}
