/*
 * The generated key definition (`keymap.generated.json`, written from
 * `crates/keymap/keymap.toml`) reaches the app as two modules, both built
 * here from that one file so neither can drift from it:
 *
 * - `virtual:mxr-key-index` (small, in the entry): the keys, aliases and
 *   scopes a label or an overlay needs. The sidebar, the topbar and the page
 *   chrome paint before the catalog arrives, so their key labels must be
 *   right in the first frame.
 * - `virtual:mxr-action-catalog` (large, loaded after the first paint): what
 *   the registry needs to be built — labels, descriptions, groups and
 *   palette data. Keys do nothing until it lands.
 *
 * `tui` is the TUI's own command mapping; the web never reads it, so it stays
 * out of the catalog module (290 kB → 94 kB) and off the wire.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

import type { Plugin } from "vite";

/** The keys and aliases a synchronous label or overlay read needs. */
export interface IndexedAction {
  id: string;
  keys?: string[];
  mac_keys?: string[];
  retired_keys?: string[];
  scopes?: string[];
  palette_only?: boolean;
  moved?: {
    key: string;
    contexts: string[];
    until: string;
    to?: string;
    to_key?: string;
  }[];
}

export interface KeyIndex {
  action: IndexedAction[];
  strip?: { id: string; actions: string[] }[];
}

/** The full definition as the web uses it: everything but the TUI's mapping. */
export interface ActionCatalog {
  action: (Omit<IndexedAction, "moved"> & {
    label: string;
    short_label?: string;
    description?: string;
    group: string;
    hide_in_palette?: boolean;
    note?: string;
    clients?: string[];
    web_native?: boolean;
    moved?: IndexedAction["moved"];
  })[];
  strip?: { id: string; actions: string[] }[];
}

export const KEY_INDEX_ID = "virtual:mxr-key-index";
export const ACTION_CATALOG_ID = "virtual:mxr-action-catalog";

interface RawAction extends IndexedAction {
  label: string;
  short_label?: string;
  description?: string;
  group: string;
  hide_in_palette?: boolean;
  note?: string;
  clients?: string[];
  web_native?: boolean;
  tui?: unknown;
}

interface RawKeymap {
  action: RawAction[];
  strip?: { id: string; actions: string[] }[];
}

const RAW = fileURLToPath(new URL("./keymap.generated.json", import.meta.url));

const INDEX_FIELDS = [
  "id",
  "keys",
  "mac_keys",
  "retired_keys",
  "scopes",
  "palette_only",
  "moved",
] as const;

function webActions(keymap: RawKeymap): RawAction[] {
  return keymap.action.filter((action) => !action.clients || action.clients.includes("web"));
}

/** Keep only the fields a side reads, in a stable order, dropping empty ones. */
function pick<T extends object>(value: T, fields: readonly (keyof T)[]): Partial<T> {
  const out: Partial<T> = {};
  for (const field of fields) {
    const item = value[field];
    if (item !== undefined) out[field] = item;
  }
  return out;
}

/** A module whose default export is the value, so Rollup can drop it when unused. */
function moduleSource(value: unknown): string {
  return `export default ${JSON.stringify(value)};\n`;
}

function indexModule(keymap: RawKeymap): string {
  const index: KeyIndex = {
    action: webActions(keymap).map((action) => pick(action, INDEX_FIELDS) as IndexedAction),
    ...(keymap.strip ? { strip: keymap.strip } : {}),
  };
  return moduleSource(index);
}

function catalogModule(keymap: RawKeymap): string {
  const catalog: ActionCatalog = {
    // Everything the web reads. `tui` is the only field left out.
    action: webActions(keymap).map(({ tui: _tui, ...action }) => action),
    ...(keymap.strip ? { strip: keymap.strip } : {}),
  };
  return moduleSource(catalog);
}

/**
 * Serve the two keymap modules. Read on first use and kept for the build, so
 * a keymap regenerated mid-build can't give the entry and the catalog
 * different data.
 */
export function keymapModules(): Plugin {
  let index: string | undefined;
  let catalog: string | undefined;
  const read = () => JSON.parse(readFileSync(RAW, "utf8")) as RawKeymap;

  return {
    name: "mxr-keymap-modules",
    resolveId(id) {
      return id === KEY_INDEX_ID || id === ACTION_CATALOG_ID ? `\0${id}` : null;
    },
    load(id) {
      if (id === `\0${KEY_INDEX_ID}`) {
        index ??= indexModule(read());
        return index;
      }
      if (id === `\0${ACTION_CATALOG_ID}`) {
        catalog ??= catalogModule(read());
        return catalog;
      }
      return null;
    },
  };
}
