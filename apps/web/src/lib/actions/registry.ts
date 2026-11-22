import { hasCommand, runCommand } from "@/lib/keys/controllers";
import { parseChord } from "@/lib/keys/chord";

import type { Action, ActionContext, ActionScope, CommandAction, ShortcutChord } from "./types";

export interface Binding {
  chord: ShortcutChord;
  scope: ActionScope;
  action: Action;
  retired: boolean;
  macOnly: boolean;
  moved: boolean;
  movedTo?: ShortcutChord;
}

export class ActionRegistry {
  #actions: Action[] = [];
  #ids = new Set<string>();
  #bindings = new Map<string, Binding>();

  define(action: Action): void {
    if (this.#ids.has(action.id)) {
      throw new Error(`ActionRegistry: duplicate id "${action.id}"`);
    }
    if (!action.paletteOnly) {
      const retired = new Set(action.retiredAliases ?? []);
      const macOnly = new Set(action.macAliases ?? []);
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
            macOnly: macOnly.has(chord),
            moved: false,
          });
        }
      }
    }
    for (const { chord, scopes, to } of action.movedAliases ?? []) {
      for (const scope of scopes) {
        const key = `${scope}|${normalize(chord)}`;
        const owner = this.#bindings.get(key);
        if (owner) {
          throw new Error(
            `ActionRegistry: moved key "${chord}" in scope "${scope}" is still bound to "${owner.action.id}"`,
          );
        }
        this.#assertNoPrefixClash(scope, chord, action.id);
        this.#bindings.set(key, {
          chord: normalize(chord),
          scope,
          action,
          retired: true,
          macOnly: false,
          moved: true,
          movedTo: to,
        });
      }
    }
    this.#ids.add(action.id);
    this.#actions.push(action);
  }

  defineMany(actions: Action[]): void {
    for (const action of actions) this.define(action);
  }

  replaceAll(actions: Action[]): void {
    const next = new ActionRegistry();
    next.defineMany(actions);
    this.#actions = next.#actions;
    this.#ids = next.#ids;
    this.#bindings = next.#bindings;
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

  resolve(chord: ShortcutChord, scopes: ActionScope[], mac = true): Binding | undefined {
    const normalized = normalize(chord);
    for (const scope of scopes) {
      const binding = this.#bindings.get(`${scope}|${normalized}`);
      if (
        binding &&
        (mac || !binding.macOnly) &&
        (binding.moved || isRunnableIn(binding.action, scope))
      ) {
        return binding;
      }
    }
    return undefined;
  }

  hasContinuation(prefix: ShortcutChord, scopes: ActionScope[], mac = true): boolean {
    const start = `${normalize(prefix)} `;
    for (const binding of this.#bindings.values()) {
      if (!scopes.includes(binding.scope) || (!mac && binding.macOnly)) continue;
      if (binding.chord.startsWith(start) && isRunnableIn(binding.action, binding.scope)) {
        return true;
      }
    }
    return false;
  }

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

export function chordsOf(action: Action, mac = true): ShortcutChord[] {
  return [
    action.shortcut,
    ...(action.aliases ?? []),
    ...(mac ? (action.macAliases ?? []) : []),
  ].filter((chord): chord is ShortcutChord => typeof chord === "string" && chord.length > 0);
}

function normalize(chord: ShortcutChord): string {
  return parseChord(chord).join(" ");
}

const BESIDE: Partial<Record<ActionScope, readonly ActionScope[]>> = {
  reader: ["list"],
  sidebar: ["list", "reader"],
  list: ["reader"],
};

function runsOn(action: CommandAction, scope: ActionScope): ActionScope | undefined {
  if (hasCommand(scope, action.command)) return scope;
  const home = scopesOf(action)[0];
  if (!home || !BESIDE[scope]?.includes(home)) return undefined;
  return hasCommand(home, action.command) ? home : undefined;
}

function isRunnableIn(action: Action, scope: ActionScope): boolean {
  if (action.command === undefined) return true;
  return runsOn(action, scope) !== undefined;
}

export function commandScope(action: Action, ctx: ActionContext): ActionScope | undefined {
  if (action.command === undefined) return undefined;
  const allowed = scopesOf(action);
  for (const scope of ctx.scopes) {
    if (!allowed.includes(scope)) continue;
    const target = runsOn(action, scope);
    if (target) return target;
  }
  return undefined;
}

export function isAvailable(action: Action, ctx: ActionContext): boolean {
  if (action.when && !action.when(ctx)) return false;
  if (action.command !== undefined) return commandScope(action, ctx) !== undefined;
  const allowed = scopesOf(action);
  return allowed.includes("global") || allowed.some((scope) => ctx.scopes.includes(scope));
}

export type InvocationSource = "key" | "palette" | "pointer";

type InvocationListener = (action: Action, source: InvocationSource) => void;

const invocationListeners = new Set<InvocationListener>();

export function onActionInvoked(listener: InvocationListener): () => void {
  invocationListeners.add(listener);
  return () => invocationListeners.delete(listener);
}

export function invokeAction(action: Action, ctx: ActionContext, source: InvocationSource): void {
  if (action.command !== undefined) {
    const scope = commandScope(action, ctx);
    if (!scope) return;
    runCommand(scope, action.command);
  } else {
    void action.run(ctx);
  }
  for (const listener of invocationListeners) listener(action, source);
}

let singleton: ActionRegistry | null = null;

export function getRegistry(): ActionRegistry {
  if (!singleton) singleton = new ActionRegistry();
  return singleton;
}

export function resetRegistry(): void {
  singleton = null;
}
