/*
 * mxr's one key definition (`crates/keymap/keymap.toml`, blueprint chapter
 * 35), as the web reads it. `keymap.generated.json` is written from the TOML
 * by `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`; every action's label, keys
 * and scopes come from there, so the TUI, `mxr keys` and the web can't
 * disagree. Feature modules declare only what an action does.
 *
 * The definition reaches the app in two pieces (see `keymapPlugin.ts`):
 *
 * - `virtual:mxr-key-index` is small and in the entry. It answers the
 *   synchronous questions a label or an overlay asks while the first screen
 *   paints (`definedShortcut`, `definedKeys`, `stripKeys`, the overlay
 *   helpers), including the person's keys from the cache.
 * - `virtual:mxr-action-catalog` is the full definition and arrives after
 *   the first contentful paint. `fromDefinition` needs it, so the action
 *   registry (the dispatcher, the palette, help and the status strip) is
 *   built when it lands; until then no key runs.
 */

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

/** The mxr release this build ships in, from the workspace `Cargo.toml`. */
export const MXR_VERSION: string = MXR_BUILD_VERSION;

/** A version's numbers; a part that isn't one counts as 0, as in `crates/keymap`. */
function versionParts(version: string): number[] {
  return version.split(".").map((part) => (/^\d+$/.test(part) ? Number(part) : 0));
}

/** True when version `a` comes before `b` (`0.6.149` < `0.6.152`), as `crates/keymap` compares them. */
export function olderThan(a: string, b: string): boolean {
  const [left, right] = [versionParts(a), versionParts(b)];
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    if (left[i] !== right[i]) return left[i]! < right[i]!;
  }
  return left.length < right.length;
}

/**
 * The definition as a build at `version` uses it: from a moved key's `until`
 * on, its notice is gone and the old key does nothing (D171). The generated
 * JSON keeps every entry, so a release that crosses an `until` leaves it
 * fresh; the TUI drops the same entries at the same version.
 */
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

/**
 * The definition's web actions by id, from the small entry index. Labels and
 * groups are not in the index: the synchronous reads below never need them.
 */
const INDEX: ReadonlyMap<string, KeyedAction> = new Map(
  withoutExpiredMoves(keyIndex.action, MXR_VERSION).map((action) => [action.id, action]),
);

/** The index with the person's keys, once the daemon or its cache has served them. */
let indexActions: ReadonlyMap<string, KeyedAction> = INDEX;

/** The full definition, once the catalog has loaded; `null` until then. */
let catalogDefault: ReadonlyMap<string, DefinedAction> | null = null;

/** The full definition with the person's keys, for `fromDefinition`. */
let catalogActions: ReadonlyMap<string, DefinedAction> | null = null;

/** The keys in force now: the last served `keys.toml`, or `null` for the definition's. */
let servedKeys: ServedKeymap | null = null;

/** True once the catalog has arrived and the registry can be built. */
export function isDefinitionLoaded(): boolean {
  return catalogDefault !== null;
}

/** Test-only: forget the full definition so `loadDefinition` loads it again. */
export function resetDefinitionForTests(): void {
  catalogDefault = null;
  catalogActions = null;
  loading = null;
}

let loading: Promise<void> | null = null;

/** Build the full definition from an already-loaded catalog module. */
export function primeDefinition(catalog: { action: DefinedAction[] }): void {
  catalogDefault = new Map(
    withoutExpiredMoves(catalog.action, MXR_VERSION).map((action) => [action.id, action]),
  );
  catalogActions = servedKeys ? withServedKeys(catalogDefault, servedKeys) : catalogDefault;
}

/**
 * Fetch the full definition (after the first paint) and apply the keys in
 * force. The index already answers synchronous reads; this is what the
 * registry needs. Resolves at once if it has already loaded.
 */
export function loadDefinition(): Promise<void> {
  if (catalogDefault) return Promise.resolve();
  loading ??= (async () => {
    const { default: catalog } = await import("virtual:mxr-action-catalog");
    primeDefinition(catalog);
  })();
  return loading;
}

/**
 * Use the daemon's keys from now on; `null` goes back to the definition's.
 * The index takes them at once, so labels are right in the next frame even
 * before the catalog lands; the catalog takes them when it does.
 */
export function setServedKeys(served: ServedKeymap | null): void {
  servedKeys = served;
  indexActions = served ? withServedKeys(INDEX, served) : INDEX;
  catalogActions = catalogDefault
    ? served
      ? withServedKeys(catalogDefault, served)
      : catalogDefault
    : null;
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
  const action = indexActions.get(id);
  if (!action?.keys?.length) {
    throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  }
  return mac ? [...action.keys, ...(action.mac_keys ?? [])] : action.keys;
}

/** The action whose key `chord` moved away from in overlay `place`, while the move is announced. */
export function movedOverlayKey(place: string, chord: string): DefinedAction | undefined {
  // The notice names the action's label, which only the catalog has. An
  // overlay opened before it lands says nothing rather than something wrong.
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

/** The full action: `behavior` plus everything the definition says about its id. */
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
      // TUI-only places in the list ("place") are not web scopes.
      scopes: contexts.filter(isScope),
      // The key to name when the action has none left (digit 1 → g i), or
      // a key on a screen that still reads its own keys.
      to: to_key ?? (to === undefined ? undefined : actions.get(to)?.keys?.[0]),
    })),
    paletteOnly: defined.palette_only,
    hideInPalette: defined.hide_in_palette,
    tuiNote: defined.note,
  };
}

/** An action's primary key, for UI outside the registry that names it. */
export function definedShortcut(id: string): string {
  const shortcut = indexActions.get(id)?.keys?.[0];
  if (!shortcut) throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  return shortcut;
}

/** Every web key an action has, primary first, for handlers outside the registry. */
export function definedKeys(id: string): string[] {
  const keys = indexActions.get(id)?.keys;
  if (!keys?.length) throw new Error(`Action "${id}" has no web key in crates/keymap/keymap.toml`);
  return keys;
}

const strips = new Map((keyIndex.strip ?? []).map((strip) => [strip.id, strip.actions]));

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

// A test imports `fromDefinition` (or the registry barrel) and builds
// actions at once, while the app waits for the catalog after the first
// paint. Prime it from the same generated module, synchronously. The build
// replaces the condition with `"production" === "test"`, so the import and
// this branch go; the catalog then only arrives through `loadDefinition`.
if (import.meta.env.MODE === "test") {
  primeDefinition(catalogData);
}
