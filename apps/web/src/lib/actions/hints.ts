/*
 * Hint selectors derived from the action registry. HelpDialog and StatusBar
 * consume these instead of the deleted shortcutHints.ts.
 *
 * Page-internal vim-style keys (j/k Move, x Select, F Fullscreen) are NOT
 * registry actions — they live in lib/pageKeyHints.ts and are merged here.
 */

import { useMemo } from "react";

import { type PageHintSection, pageHintsForRoute } from "@/lib/pageKeyHints";

import "./catalog";
import { getRegistry } from "./registry";
import type { Action, ActionContext, ActionGroup, ShortcutChord } from "./types";

export interface ShortcutHint {
  key: string;
  label: string;
}

export interface ShortcutSection {
  title: string;
  hints: ShortcutHint[];
}

const GROUP_TITLE: Record<ActionGroup, string> = {
  Navigate: "Navigation",
  Mail: "Mail",
  Compose: "Compose",
  Search: "Search",
  Triage: "Triage",
  Semantic: "Semantic",
  Accounts: "Accounts",
  Settings: "Settings",
  Diagnostics: "Diagnostics",
  Rules: "Rules",
  Analytics: "Analytics",
  View: "View",
};

const GROUP_ORDER: ActionGroup[] = [
  "Navigate",
  "Mail",
  "Search",
  "Compose",
  "Triage",
  "Analytics",
  "Rules",
  "Accounts",
  "Semantic",
  "Diagnostics",
  "Settings",
  "View",
];

export function actionShortcutSections(
  ctx: ActionContext,
  pageSections: PageHintSection[] = [],
): ShortcutSection[] {
  const reg = getRegistry();
  const visible = reg
    .getVisibleActions(ctx)
    .filter((a) => a.shortcut !== undefined);

  const grouped = new Map<ActionGroup, ShortcutHint[]>();
  for (const action of visible) {
    const hint = { key: formatChord(action.shortcut!), label: action.label };
    const existing = grouped.get(action.group);
    if (existing) {
      existing.push(hint);
    } else {
      grouped.set(action.group, [hint]);
    }
  }

  const sections: ShortcutSection[] = [];
  for (const group of GROUP_ORDER) {
    const hints = grouped.get(group);
    if (hints && hints.length > 0) {
      sections.push({ title: GROUP_TITLE[group], hints });
    }
  }
  for (const page of pageSections) {
    sections.push({ title: page.title, hints: page.hints });
  }
  return sections;
}

export function useActionShortcutSections(ctx: ActionContext): ShortcutSection[] {
  const pageSections = pageHintsForRoute(ctx);
  return useMemo(
    () => actionShortcutSections(ctx, pageSections),
    [ctx, pageSections],
  );
}

export function useActionPrimaryHints(ctx: ActionContext, limit = 5): ShortcutHint[] {
  const sections = useActionShortcutSections(ctx);
  return useMemo(() => sections[0]?.hints.slice(0, limit) ?? [], [sections, limit]);
}

export function useVisibleActions(ctx: ActionContext): Action[] {
  return useMemo(() => getRegistry().getVisibleActions(ctx), [ctx]);
}

export function useActionsByGroup(ctx: ActionContext): Map<ActionGroup, Action[]> {
  const visible = useVisibleActions(ctx);
  return useMemo(() => {
    const map = new Map<ActionGroup, Action[]>();
    for (const action of visible) {
      const existing = map.get(action.group);
      if (existing) {
        existing.push(action);
      } else {
        map.set(action.group, [action]);
      }
    }
    return map;
  }, [visible]);
}

/** Renders tinykeys grammar back into a human-readable chip label. */
export function formatChord(chord: ShortcutChord): string {
  return chord
    .replace(/Shift\+Slash/g, "?")
    .replace(/Shift\+Semicolon/g, ":")
    .replace(/Key([A-Z])/g, (_, letter) => letter.toLowerCase())
    .replace(/Digit(\d)/g, (_, digit) => digit)
    .replace(/Slash/g, "/")
    .replace(/\$mod\+/g, "⌘")
    .replace(/\$mod/g, "⌘");
}

export type { Action };
