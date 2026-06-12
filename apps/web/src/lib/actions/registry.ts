/*
 * ActionRegistry — single source of truth for the command palette, global
 * keymap, and help dialog. Define-time validation guards against duplicate ids
 * and shortcut collisions (the `g a` bug we're closing).
 */

import type { Action, ActionContext, ActionScope, ShortcutChord } from "./types";

export class ActionRegistry {
  #actions: Action[] = [];
  #ids = new Set<string>();
  /** "<scope>:<chord>" → action id. Scoped uniqueness; displayOnly entries
   * also reserve their chord so a real binding can't shadow a page key. */
  #shortcuts = new Map<string, string>();

  define(action: Action): void {
    if (this.#ids.has(action.id)) {
      throw new Error(`ActionRegistry: duplicate id "${action.id}"`);
    }
    if (!action.paletteOnly) {
      const scope = action.scope ?? "global";
      const allChords: ShortcutChord[] = [];
      if (action.shortcut) allChords.push(action.shortcut);
      if (action.aliases) allChords.push(...action.aliases);
      for (const chord of allChords) {
        const key = `${scope}:${chord}`;
        const owner = this.#shortcuts.get(key);
        if (owner) {
          throw new Error(
            `ActionRegistry: duplicate shortcut "${chord}" in scope "${scope}" (already bound to "${owner}")`,
          );
        }
        this.#shortcuts.set(key, action.id);
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

  getActionForShortcut(chord: ShortcutChord, scope: ActionScope = "global"): Action | undefined {
    const id = this.#shortcuts.get(`${scope}:${chord}`) ?? this.#shortcuts.get(`global:${chord}`);
    if (!id) return undefined;
    return this.get(id);
  }

  getVisibleActions(ctx: ActionContext): Action[] {
    return this.#actions.filter((a) => !a.when || a.when(ctx));
  }

  /**
   * Returns chord → candidate action ids by scope, omitting paletteOnly and
   * displayOnly entries. The keymap resolves the winner at dispatch time:
   * active scope first, then global.
   */
  getShortcutMap(): Record<ShortcutChord, Partial<Record<ActionScope, string>>> {
    const map: Record<ShortcutChord, Partial<Record<ActionScope, string>>> = {};
    for (const [key, id] of this.#shortcuts) {
      const action = this.get(id);
      if (action?.displayOnly) continue;
      const sep = key.indexOf(":");
      const scope = key.slice(0, sep) as ActionScope;
      const chord = key.slice(sep + 1);
      (map[chord] ??= {})[scope] = id;
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
