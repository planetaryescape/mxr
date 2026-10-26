import { isAvailable, type ActionRegistry } from "@/lib/actions/registry";
import type { ActionContext } from "@/lib/actions/types";

/** Only the bindings the dispatcher would actually choose in this context. */
export function shortcutContinuations(
  registry: ActionRegistry,
  prefix: string,
  context: ActionContext,
  mac: boolean,
) {
  const start = `${prefix} `;
  return registry
    .bindings()
    .flatMap((binding) => {
      if (!binding.chord.startsWith(start) || binding.moved || binding.retired) return [];
      if (
        registry.resolve(binding.chord, context.scopes, mac) !== binding ||
        !isAvailable(binding.action, context)
      )
        return [];
      return [
        {
          chord: binding.chord,
          next: binding.chord.slice(start.length),
          label: binding.action.shortLabel ?? binding.action.label,
          priority: context.scopes.indexOf(binding.scope),
        },
      ];
    })
    .toSorted((a, b) => a.priority - b.priority)
    .map(({ chord, next, label }) => ({ chord, next, label }));
}
