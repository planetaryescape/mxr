import { isAvailable, type ActionRegistry } from "@/lib/actions/registry";
import type { ActionContext } from "@/lib/actions/types";

export interface Continuation {
  chord: string;
  next: string[];
  label: string;
}

function keyClass(next: string): number {
  if (/^[a-z]$/.test(next)) return 0;
  if (/^[A-Z]$/.test(next)) return 2;
  if (/^[0-9]$/.test(next)) return 3;
  return 1;
}

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
        a.priority - b.priority || keyClass(a.next[0]!) - keyClass(b.next[0]!) || a.order - b.order,
    )
    .map(({ chord, next, label }) => ({ chord, next, label }));
}
