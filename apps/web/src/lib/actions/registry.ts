/*
 * ActionRegistry — single source of truth for the command palette, global
 * keymap, and help dialog. Define-time validation guards against duplicate ids
 * and shortcut collisions (the `g a` bug we're closing).
 */

import type { Action, ActionContext, ShortcutChord } from "./types";

export class ActionRegistry {
  #actions: Action[] = [];
  #ids = new Set<string>();
  #shortcuts = new Map<ShortcutChord, string>();

  define(action: Action): void {
    if (this.#ids.has(action.id)) {
      throw new Error(`ActionRegistry: duplicate id "${action.id}"`);
    }
    if (!action.paletteOnly) {
      const allChords: ShortcutChord[] = [];
      if (action.shortcut) allChords.push(action.shortcut);
      if (action.aliases) allChords.push(...action.aliases);
      for (const chord of allChords) {
        const owner = this.#shortcuts.get(chord);
        if (owner) {
          throw new Error(
            `ActionRegistry: duplicate shortcut "${chord}" (already bound to "${owner}")`,
          );
        }
        this.#shortcuts.set(chord, action.id);
      }
    }
    this.#ids.add(action.id);
    this.#actions.push(action);
  }

  defineMany(actions: Action[]): void {
    for (const a of actions) this.define(a);
  }

  all(): readonly Action[] {
    return this.#actions;
  }

  get(id: string): Action | undefined {
    return this.#actions.find((a) => a.id === id);
  }

  getActionForShortcut(chord: ShortcutChord): Action | undefined {
    const id = this.#shortcuts.get(chord);
    if (!id) return undefined;
    return this.get(id);
  }

  getVisibleActions(ctx: ActionContext): Action[] {
    return this.#actions.filter((a) => !a.when || a.when(ctx));
  }

  /** Returns chord → action id, omitting paletteOnly entries. Includes aliases. */
  getShortcutMap(): Record<ShortcutChord, string> {
    const map: Record<ShortcutChord, string> = {};
    for (const [chord, id] of this.#shortcuts) {
      map[chord] = id;
    }
    return map;
  }
}

let singleton: ActionRegistry | null = null;

export function getRegistry(): ActionRegistry {
  if (!singleton) singleton = new ActionRegistry();
  return singleton;
}

/** Test-only — reset the module-level registry between specs. */
export function resetRegistry(): void {
  singleton = null;
}
