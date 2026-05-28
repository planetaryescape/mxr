/*
 * Shared action registry types. Pure types — zero React imports.
 *
 * The registry is consumed by the command palette, the global keymap, and the
 * help dialog. Each surface filters by `when` and groups by `group`. Durable
 * app-level actions live here; page-local motion and terminal-only TUI state
 * stay close to the view that owns them.
 */

import type { ComponentType } from "react";

import type { MailPane } from "@/state/mailboxPaneStore";

export type ActionGroup =
  | "Mail"
  | "Compose"
  | "Search"
  | "Semantic"
  | "Accounts"
  | "Navigate"
  | "Settings"
  | "Diagnostics"
  | "Rules"
  | "Analytics"
  | "Triage"
  | "View";

/** tinykeys grammar — e.g. "g a", "$mod+KeyK", "Shift+Slash". */
export type ShortcutChord = string;

export interface ActionContext {
  path: string;
  activePane: MailPane;
  selectionCount: number;
  accountCount: number;
  hasFocusedThread: boolean;
  hasFocusedMessage: boolean;
  isFirstAccountOnly: boolean;
}

export type ActionRunner = (ctx: ActionContext) => void | Promise<void>;

export type ActionPredicate = (ctx: ActionContext) => boolean;

type IconComponent = ComponentType<{ className?: string }>;

export interface Action {
  id: string;
  label: string;
  description?: string;
  group: ActionGroup;
  icon?: IconComponent;
  shortcut?: ShortcutChord;
  /** Additional chords that bind to the same action (e.g. numeric quick-nav). */
  aliases?: ShortcutChord[];
  /** When true, action does not bind to the global keymap even if `shortcut` is set. */
  paletteOnly?: boolean;
  when?: ActionPredicate;
  run: ActionRunner;
}
