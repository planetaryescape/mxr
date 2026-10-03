/*
 * Help, palette and status-bar views of the registry. Help groups bindings
 * by where they work, current view first, so what it shows is exactly
 * what the dispatcher will do.
 */

import { useMemo } from "react";

import { formatChord } from "@/lib/keys/chord";

import "./catalog";
import { chordsOf, getRegistry, isAvailable, scopesOf } from "./registry";
import type { Action, ActionContext, ActionGroup, ActionScope } from "./types";

export interface ShortcutHint {
  id: string;
  keys: string[];
  label: string;
  /** What the status bar shows. */
  shortLabel: string;
  note?: string;
  /** Available right now (scope mounted, predicate true). */
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
  place: "Updates and Reading",
  todo: "To do",
  catchup: "To do: catch-up",
  expired: "To do: Expired list",
  now: "Now",
};

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
    keys: chordsOf(action).map((chord) => formatChord(chord)),
    label: action.label,
    shortLabel: action.shortLabel ?? action.label.replace(/…$/, ""),
    note: action.tuiNote,
    live: isAvailable(action, ctx),
  };
}

/**
 * Sections for the help dialog: the active view's keys first, then mail
 * verbs, then global keys, then the other views' keys for discovery.
 */
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
      continue;
    }
    for (const scope of scopes) byScope.set(scope, [...(byScope.get(scope) ?? []), action]);
  }

  const sections: ShortcutSection[] = [];
  const paneScopes = (["reader", "list", "sidebar", "screener", "focus"] as const).filter((scope) =>
    byScope.has(scope),
  );
  const activePanes = paneScopes.filter((scope) => ctx.scopes.includes(scope));
  const otherPanes = paneScopes.filter((scope) => !ctx.scopes.includes(scope));

  for (const scope of activePanes) {
    sections.push({
      id: scope,
      title: `${SCOPE_TITLE[scope]} (this view)`,
      hints: (byScope.get(scope) ?? []).map((action) => hint(action, ctx)),
    });
  }
  if (mailVerbs.length > 0) {
    sections.push({
      id: "mail",
      title: "Mail actions",
      hints: mailVerbs.map((action) => hint(action, ctx)),
    });
  }

  const globalGroups = new Map<string, ShortcutHint[]>();
  for (const action of byScope.get("global") ?? []) {
    const title = GLOBAL_GROUP_TITLE[action.group] ?? "Everywhere";
    globalGroups.set(title, [...(globalGroups.get(title) ?? []), hint(action, ctx)]);
  }
  for (const title of ["Everywhere", "Search", "Go to"]) {
    const hints = globalGroups.get(title);
    if (hints) sections.push({ id: `global-${title}`, title, hints });
  }

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
  return useMemo(() => shortcutSections(ctx), [ctx]);
}

/** A few live hints for the status bar: the current view's keys first. */
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

export function useVisibleActions(ctx: ActionContext): Action[] {
  return useMemo(() => getRegistry().getVisibleActions(ctx), [ctx]);
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
