import keyIndex from "virtual:mxr-key-index";
import catalogData from "virtual:mxr-action-catalog";

import type { Action, ActionBehavior, ActionGroup, ActionScope } from "./types";

export interface KeyedAction {
  id: string;
  keys?: string[];
  mac_keys?: string[];
  retired_keys?: string[];
  scopes?: string[];
  palette_only?: boolean;
  moved?: { key: string; contexts: string[]; until: string; to?: string; to_key?: string }[];
}

export interface DefinedAction extends KeyedAction {
  label: string;
  short_label?: string;
  description?: string;
  group: string;
  hide_in_palette?: boolean;
  note?: string;
  clients?: string[];
  web_native?: boolean;
}

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

export const MXR_VERSION: string = MXR_BUILD_VERSION;

function versionParts(version: string): number[] {
  return version.split(".").map((part) => (/^\d+$/.test(part) ? Number(part) : 0));
}

export function olderThan(a: string, b: string): boolean {
  const [left, right] = [versionParts(a), versionParts(b)];
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    if (left[i] !== right[i]) return left[i]! < right[i]!;
  }
  return left.length < right.length;
}

export function withoutExpiredMoves<T extends Pick<DefinedAction, "moved">>(
  actions: readonly T[],
  version: string,
): T[] {
  return actions.map((action) => {
    if (!action.moved?.length) return action;
    const moved = action.moved.filter((move) => olderThan(version, move.until));
    return moved.length === action.moved.length ? action : { ...action, moved };
  });
}

export interface ServedKeymap {
  actions: { id: string; keys?: string[]; mac_keys?: string[] }[];
  single_keys?: boolean;
  remapped?: string[];
  problems?: string[];
}

export function withServedKeys<T extends KeyedAction>(
  defined: ReadonlyMap<string, T>,
  served: ServedKeymap,
): Map<string, T> {
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

const INDEX: ReadonlyMap<string, KeyedAction> = new Map(
  withoutExpiredMoves(keyIndex.action, MXR_VERSION).map((action) => [action.id, action]),
);

let indexActions: ReadonlyMap<string, KeyedAction> = INDEX;

let catalogDefault: ReadonlyMap<string, DefinedAction> | null = null;

let catalogActions: ReadonlyMap<string, DefinedAction> | null = null;

let servedKeys: ServedKeymap | null = null;

export function isDefinitionLoaded(): boolean {
  return catalogDefault !== null;
}

export function resetDefinitionForTests(): void {
  catalogDefault = null;
  catalogActions = null;
  loading = null;
}

let loading: Promise<void> | null = null;

export function primeDefinition(catalog: { action: DefinedAction[] }): void {
  catalogDefault = new Map(
    withoutExpiredMoves(catalog.action, MXR_VERSION).map((action) => [action.id, action]),
  );
  catalogActions = servedKeys ? withServedKeys(catalogDefault, servedKeys) : catalogDefault;
}

export function loadDefinition(): Promise<void> {
  if (catalogDefault) return Promise.resolve();
  loading ??= (async () => {
    const { default: catalog } = await import("virtual:mxr-action-catalog");
    primeDefinition(catalog);
  })();
  return loading;
}

export function setServedKeys(served: ServedKeymap | null): void {
  servedKeys = served;
  indexActions = served ? withServedKeys(INDEX, served) : INDEX;
  catalogActions = catalogDefault
    ? served
      ? withServedKeys(catalogDefault, served)
      : catalogDefault
    : null;
}

export function isOverlayKey(action: Pick<DefinedAction, "scopes">): boolean {
  const scopes = action.scopes ?? [];
  return scopes.length > 0 && scopes.every((scope) => !isScope(scope));
}

export function definedOverlayKeys(id: string, mac: boolean): string[] {
  const action = indexActions.get(id);
  if (!action?.keys?.length) {
    throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  }
  return mac ? [...action.keys, ...(action.mac_keys ?? [])] : action.keys;
}

export function movedOverlayKey(place: string, chord: string): DefinedAction | undefined {
  if (!catalogActions) return undefined;
  for (const action of catalogActions.values()) {
    if (action.moved?.some((move) => move.key === chord && move.contexts.includes(place))) {
      return action;
    }
  }
  return undefined;
}

function isGroup(group: string): group is ActionGroup {
  return Object.hasOwn(GROUPS, group);
}

function isScope(scope: string): scope is ActionScope {
  return Object.hasOwn(SCOPES, scope);
}

export function fromDefinition(behavior: ActionBehavior): Action {
  const { id } = behavior;
  const actions = catalogActions;
  if (!actions) {
    throw new Error(
      "mxr: the action catalog has not loaded yet; await loadDefinition() before building the registry",
    );
  }
  const defined = actions.get(id);
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
      scopes: contexts.filter(isScope),
      to: to_key ?? (to === undefined ? undefined : actions.get(to)?.keys?.[0]),
    })),
    paletteOnly: defined.palette_only,
    hideInPalette: defined.hide_in_palette,
    tuiNote: defined.note,
  };
}

export function definedShortcut(id: string): string {
  const shortcut = indexActions.get(id)?.keys?.[0];
  if (!shortcut) throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  return shortcut;
}

export function definedKeys(id: string): string[] {
  const keys = indexActions.get(id)?.keys;
  if (!keys?.length) throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  return keys;
}

const strips = new Map((keyIndex.strip ?? []).map((strip) => [strip.id, strip.actions]));

export function stripKeys(id: string, omit: readonly string[] = []): string[] {
  const actions = strips.get(id);
  if (!actions) throw new Error(`No strip "${id}" in crates/keymap/keymap.toml`);
  return actions.filter((action) => !omit.includes(action));
}

if (import.meta.env.MODE === "test") {
  primeDefinition(catalogData);
}
