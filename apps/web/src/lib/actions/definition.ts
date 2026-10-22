/*
 * mxr's one key definition (`crates/keymap/keymap.toml`, blueprint chapter
 * 35), as the web reads it. `keymap.generated.json` is written from the TOML
 * by `UPDATE_KEYMAP=1 cargo test -p mxr-keymap`; every action's label, keys
 * and scopes come from there, so the TUI, `mxr keys` and the web can't
 * disagree. Feature modules declare only what an action does.
 */

import keymap from "./keymap.generated.json";
import type { Action, ActionBehavior, ActionGroup, ActionScope } from "./types";

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
  updates: true,
  archive: true,
  reading: true,
  "reading-reader": true,
};

/** The definition's web actions by id. Actions without "web" in `clients` are the TUI's alone. */
const webActions = new Map(
  (keymap.action as DefinedAction[])
    .filter((action) => !action.clients || action.clients.includes("web"))
    .map((action) => [action.id, action]),
);

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
