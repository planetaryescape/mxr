import { useMemo } from "react";

import { formatChord, isMacPlatform } from "@/lib/keys/chord";
import { useKeymapStore } from "@/state/keymapStore";

import "./catalog";
import { chordsOf, getRegistry, isAvailable, scopesOf } from "./registry";
import type { Action, ActionContext, ActionGroup, ActionScope } from "./types";

export interface ShortcutHint {
  id: string;
  keys: string[];
  label: string;
  shortLabel: string;
  note?: string;
  live: boolean;
}

export interface ShortcutSection {
  id: string;
  title: string;
  hints: ShortcutHint[];
}

const SCOPE_TITLE: Record<Exclude<ActionScope, "global">, string> = {
  list: "Mail list",
  reader: "Reader",
  sidebar: "Sidebar",
  screener: "Screener",
  focus: "Focus & reply",
  todo: "To do",
  catchup: "To do: catch-up",
  expired: "To do: Expired list",
  now: "Now",
  messages: "Messages",
  "messages-filters": "Messages filters",
  page: "Page controls",
  archive: "Archive",
  updates: "Updates",
  reading: "Reading",
  "reading-reader": "Reading: the reader",
  compose: "Compose",
  arrivals: "Arrivals",
};

const PANE_ORDER: readonly string[] = ["reader", "list", "sidebar", "screener", "focus"];

const GLOBAL_GROUP_TITLE: Partial<Record<ActionGroup, string>> = {
  Navigate: "Go to",
  Triage: "Go to",
  Analytics: "Go to",
  Diagnostics: "Go to",
  Rules: "Go to",
  Accounts: "Go to",
  Search: "Search",
  Compose: "Everywhere",
  Mail: "Everywhere",
  Settings: "Go to",
};

function hint(action: Action, ctx: ActionContext): ShortcutHint {
  return {
    id: action.id,
    keys: chordsOf(action, isMacPlatform()).map((chord) => formatChord(chord)),
    label: action.label,
    shortLabel: action.shortLabel ?? action.label.replace(/…$/, ""),
    note: action.tuiNote,
    live: isAvailable(action, ctx),
  };
}

export function shortcutSections(ctx: ActionContext): ShortcutSection[] {
  const bound = getRegistry()
    .all()
    .filter((action) => !action.paletteOnly && chordsOf(action).length > 0);

  const byScope = new Map<ActionScope, Action[]>();
  const mailVerbs: Action[] = [];
  for (const action of bound) {
    const scopes = scopesOf(action);
    if (scopes.includes("list") && scopes.includes("reader")) {
      mailVerbs.push(action);
      for (const scope of scopes) {
        if (scope === "list" || scope === "reader") continue;
        byScope.set(scope, [...(byScope.get(scope) ?? []), action]);
      }
      continue;
    }
    for (const scope of scopes) byScope.set(scope, [...(byScope.get(scope) ?? []), action]);
  }

  const sections: ShortcutSection[] = [];
  const paneScopes = (Object.keys(SCOPE_TITLE) as (keyof typeof SCOPE_TITLE)[])
    .toSorted((a, b) => PANE_ORDER.indexOf(a) - PANE_ORDER.indexOf(b))
    .filter((scope) => byScope.has(scope));
  const activePanes = paneScopes.filter((scope) => ctx.scopes.includes(scope));
  const otherPanes = paneScopes.filter((scope) => !ctx.scopes.includes(scope));

  for (const scope of activePanes) {
    sections.push({
      id: scope,
      title: `${SCOPE_TITLE[scope]} (this view)`,
      hints: (byScope.get(scope) ?? []).map((action) => hint(action, ctx)),
    });
  }
  const mailSection: ShortcutSection | null =
    mailVerbs.length > 0
      ? { id: "mail", title: "Mail actions", hints: mailVerbs.map((action) => hint(action, ctx)) }
      : null;
  const mailHere = ctx.scopes.includes("list") || ctx.scopes.includes("reader");
  if (mailSection && mailHere) sections.push(mailSection);

  const globalGroups = new Map<string, ShortcutHint[]>();
  for (const action of byScope.get("global") ?? []) {
    const title = GLOBAL_GROUP_TITLE[action.group] ?? "Everywhere";
    globalGroups.set(title, [...(globalGroups.get(title) ?? []), hint(action, ctx)]);
  }
  for (const title of ["Everywhere", "Search", "Go to"]) {
    const hints = globalGroups.get(title);
    if (hints) sections.push({ id: `global-${title}`, title, hints });
  }

  if (mailSection && !mailHere) sections.push(mailSection);
  for (const scope of otherPanes) {
    sections.push({
      id: scope,
      title: SCOPE_TITLE[scope],
      hints: (byScope.get(scope) ?? []).map((action) => hint(action, ctx)),
    });
  }
  return sections;
}

export function useShortcutSections(ctx: ActionContext): ShortcutSection[] {
  const version = useKeymapStore((state) => state.version);
  // eslint-disable-next-line react-hooks/exhaustive-deps -- the registry is rebuilt when version changes
  return useMemo(() => shortcutSections(ctx), [ctx, version]);
}

export function useActionPrimaryHints(ctx: ActionContext, limit = 5): ShortcutHint[] {
  const sections = useShortcutSections(ctx);
  return useMemo(
    () =>
      sections
        .flatMap((section) => section.hints)
        .filter((item) => item.live)
        .slice(0, limit),
    [sections, limit],
  );
}

export function pageKeyHints(ids: readonly string[], ctx: ActionContext): ShortcutHint[] {
  const registry = getRegistry();
  return ids.flatMap((id) => {
    const action = registry.get(id);
    return action && chordsOf(action).length > 0 ? [hint(action, ctx)] : [];
  });
}

export function useVisibleActions(ctx: ActionContext): Action[] {
  const version = useKeymapStore((state) => state.version);
  // eslint-disable-next-line react-hooks/exhaustive-deps -- the registry is rebuilt when version changes
  return useMemo(() => getRegistry().getVisibleActions(ctx), [ctx, version]);
}

export function useActionsByGroup(ctx: ActionContext): Map<ActionGroup, Action[]> {
  const visible = useVisibleActions(ctx);
  return useMemo(() => {
    const map = new Map<ActionGroup, Action[]>();
    for (const action of visible) {
      if (action.hideInPalette) continue;
      map.set(action.group, [...(map.get(action.group) ?? []), action]);
    }
    return map;
  }, [visible]);
}

export { formatChord };
export type { Action };
