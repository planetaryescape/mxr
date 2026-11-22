import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

import type { Plugin } from "vite";

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

function pick<T extends object>(value: T, fields: readonly (keyof T)[]): Partial<T> {
  const out: Partial<T> = {};
  for (const field of fields) {
    const item = value[field];
    if (item !== undefined) out[field] = item;
  }
  return out;
}

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
    action: webActions(keymap).map(({ tui: _tui, ...action }) => action),
    ...(keymap.strip ? { strip: keymap.strip } : {}),
  };
  return moduleSource(catalog);
}

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
