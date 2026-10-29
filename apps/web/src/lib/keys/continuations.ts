import { isAvailable, type ActionRegistry } from "@/lib/actions/registry";
import type { ActionContext } from "@/lib/actions/types";

export interface Continuation {
  /** The primary chord, e.g. "g i". */
  chord: string;
  /** Every next key that runs this action here, primary first ("i", "0"). */
  next: string[];
  label: string;
}

/**
 * The order the guide lists next keys in, after scope precedence: blueprint
 * 35's `g` table puts mail places and modes (lowercase) before tools
 * (capitals), and saved searches (digits) last. Without it the guide's first
 * rows were whatever the registry defined first, and To do and Reading fell
 * past its cut.
 */
function keyClass(next: string): number {
  if (/^[a-z]$/.test(next)) return 0;
  if (/^[A-Z]$/.test(next)) return 2;
  if (/^[0-9]$/.test(next)) return 3;
  return 1;
}

/** Only the bindings the dispatcher would actually choose in this context, one row per action. */
export function shortcutContinuations(
  registry: ActionRegistry,
  prefix: string,
  context: ActionContext,
  mac: boolean,
): Continuation[] {
  const start = `${prefix} `;
  const rows = new Map<string, Continuation & { priority: number; order: number }>();
  for (const binding of registry.bindings()) {
    if (!binding.chord.startsWith(start) || binding.moved || binding.retired) continue;
    if (
      registry.resolve(binding.chord, context.scopes, mac) !== binding ||
      !isAvailable(binding.action, context)
    )
      continue;
    const next = binding.chord.slice(start.length);
    const row = rows.get(binding.action.id);
    if (row) {
      // An alias (`g 0` beside `g i`) joins its action's row instead of taking one.
      row.next.push(next);
      continue;
    }
    rows.set(binding.action.id, {
      chord: binding.chord,
      next: [next],
      label: binding.action.shortLabel ?? binding.action.label,
      priority: context.scopes.indexOf(binding.scope),
      order: rows.size,
    });
  }
  return [...rows.values()]
    .toSorted(
      (a, b) =>
        a.priority - b.priority ||
        keyClass(a.next[0]!) - keyClass(b.next[0]!) ||
        a.order - b.order,
    )
    .map(({ chord, next, label }) => ({ chord, next, label }));
}
