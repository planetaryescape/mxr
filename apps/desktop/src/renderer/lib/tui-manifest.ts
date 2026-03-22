import { tuiDesktopManifest } from "../../shared/generated/tui-manifest";

export type DesktopBindingContext = keyof typeof tuiDesktopManifest.bindings;
type ManifestBindings = typeof tuiDesktopManifest.bindings;
export type DesktopBinding = ManifestBindings[keyof ManifestBindings][number];
export type DesktopCommand = (typeof tuiDesktopManifest.commands)[number];
export type DesktopAction = DesktopBinding["action"] | DesktopCommand["action"];

export type PendingBinding = {
  tokens: string[];
  deadline: number;
};

export function bindingsForContext(context: DesktopBindingContext): readonly DesktopBinding[] {
  return tuiDesktopManifest.bindings[context];
}

export function commandPaletteEntries(): readonly DesktopCommand[] {
  return tuiDesktopManifest.commands;
}

export function resolveBindingAction(
  context: DesktopBindingContext,
  token: string,
  pending: PendingBinding | null,
  now: number,
): { action?: DesktopAction; pending?: PendingBinding | null } | null {
  const bindings = bindingsForContext(context);
  const activeSequence = pending && pending.deadline >= now ? [...pending.tokens, token] : [token];

  const exact = bindings.find((binding) => matchesTokens(binding.tokens, activeSequence));
  if (exact) {
    return { action: exact.action, pending: null };
  }

  const partial = bindings.some((binding) => isPrefix(activeSequence, binding.tokens));
  if (partial) {
    return {
      pending: {
        tokens: activeSequence,
        deadline: now + 500,
      },
    };
  }

  return null;
}

function matchesTokens(left: readonly string[], right: readonly string[]) {
  return left.length === right.length && left.every((token, index) => token === right[index]);
}

function isPrefix(prefix: readonly string[], full: readonly string[]) {
  return prefix.length < full.length && prefix.every((token, index) => token === full[index]);
}
