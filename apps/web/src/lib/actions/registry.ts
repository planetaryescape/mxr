/*
 * ActionRegistry: the single table behind the palette, the key dispatcher
 * and help. Define-time validation rejects duplicate ids and a chord bound
 * twice in one scope, and a chord that is a prefix of another chord in the
 * same scope (which would make the shorter one unreachable).
 */

import { hasCommand, runCommand } from "@/lib/keys/controllers";
import { parseChord } from "@/lib/keys/chord";

import type { Action, ActionContext, ActionScope, ShortcutChord } from "./types";

export interface Binding {
  chord: ShortcutChord;
  scope: ActionScope;
  action: Action;
  /** Bound from `retiredAliases`: works, but is not advertised. */
  retired: boolean;
}

export class ActionRegistry {
  #actions: Action[] = [];
  #ids = new Set<string>();
  /** "<scope>|<chord>" → binding. */
  #bindings = new Map<string, Binding>();

  define(action: Action): void {
    if (this.#ids.has(action.id)) {
      throw new Error(`ActionRegistry: duplicate id "${action.id}"`);
    }
    if (!action.paletteOnly) {
      const retired = new Set(action.retiredAliases ?? []);
      for (const scope of scopesOf(action)) {
        for (const chord of [...chordsOf(action), ...retired]) {
          const key = `${scope}|${normalize(chord)}`;
          const owner = this.#bindings.get(key);
          if (owner) {
            throw new Error(
              `ActionRegistry: duplicate shortcut "${chord}" in scope "${scope}" (already bound to "${owner.action.id}")`,
            );
          }
          this.#assertNoPrefixClash(scope, chord, action.id);
          this.#bindings.set(key, {
            chord: normalize(chord),
            scope,
            action,
            retired: retired.has(chord),
          });
        }
      }
    }
    this.#ids.add(action.id);
    this.#actions.push(action);
  }

  defineMany(actions: Action[]): void {
    for (const action of actions) this.define(action);
  }

  all(): readonly Action[] {
    return this.#actions;
  }

  get(id: string): Action | undefined {
    return this.#actions.find((action) => action.id === id);
  }

  bindings(): Binding[] {
    return [...this.#bindings.values()];
  }

  /**
   * The action a chord triggers given the active scopes (innermost first).
   * Scoped bindings only count when their controller implements the
   * command, so an unmounted view can't swallow a key.
   */
  resolve(chord: ShortcutChord, scopes: ActionScope[]): Binding | undefined {
    const normalized = normalize(chord);
    for (const scope of scopes) {
      const binding = this.#bindings.get(`${scope}|${normalized}`);
      if (binding && isRunnableIn(binding.action, scope)) return binding;
    }
    return undefined;
  }

  /** True when some live binding continues the typed sequence. */
  hasContinuation(prefix: ShortcutChord, scopes: ActionScope[]): boolean {
    const start = `${normalize(prefix)} `;
    for (const binding of this.#bindings.values()) {
      if (!scopes.includes(binding.scope)) continue;
      if (binding.chord.startsWith(start) && isRunnableIn(binding.action, binding.scope)) {
        return true;
      }
    }
    return false;
  }

  /** Actions the palette and help can offer in this context. */
  getVisibleActions(ctx: ActionContext): Action[] {
    return this.#actions.filter((action) => isAvailable(action, ctx));
  }

  #assertNoPrefixClash(scope: ActionScope, chord: ShortcutChord, id: string): void {
    const normalized = normalize(chord);
    for (const binding of this.#bindings.values()) {
      if (binding.scope !== scope) continue;
      const clash =
        binding.chord.startsWith(`${normalized} `) || normalized.startsWith(`${binding.chord} `);
      if (clash) {
        throw new Error(
          `ActionRegistry: chord "${chord}" (${id}) clashes with "${binding.chord}" (${binding.action.id}) in scope "${scope}"`,
        );
      }
    }
  }
}

export function scopesOf(action: Action): ActionScope[] {
  return action.scopes && action.scopes.length > 0 ? action.scopes : ["global"];
}

export function chordsOf(action: Action): ShortcutChord[] {
  return [action.shortcut, ...(action.aliases ?? [])].filter(
    (chord): chord is ShortcutChord => typeof chord === "string" && chord.length > 0,
  );
}

function normalize(chord: ShortcutChord): string {
  return parseChord(chord).join(" ");
}

function isRunnableIn(action: Action, scope: ActionScope): boolean {
  if (action.command === undefined) return true;
  return hasCommand(scope, action.command);
}

/** The scope a command action would run in, or undefined if none is live. */
export function commandScope(action: Action, ctx: ActionContext): ActionScope | undefined {
  if (action.command === undefined) return undefined;
  const allowed = scopesOf(action);
  return ctx.scopes.find((scope) => allowed.includes(scope) && hasCommand(scope, action.command!));
}

export function isAvailable(action: Action, ctx: ActionContext): boolean {
  if (action.when && !action.when(ctx)) return false;
  if (action.command !== undefined) return commandScope(action, ctx) !== undefined;
  const allowed = scopesOf(action);
  return allowed.includes("global") || allowed.some((scope) => ctx.scopes.includes(scope));
}

/** Run an action from any surface (key, palette, button). */
export function invokeAction(action: Action, ctx: ActionContext): void {
  if (action.command !== undefined) {
    const scope = commandScope(action, ctx);
    if (scope) runCommand(scope, action.command);
    return;
  }
  void action.run(ctx);
}

let singleton: ActionRegistry | null = null;

export function getRegistry(): ActionRegistry {
  if (!singleton) singleton = new ActionRegistry();
  return singleton;
}

/** Test-only: reset the module-level registry between specs. */
export function resetRegistry(): void {
  singleton = null;
}
