/*
 * Shared action registry types. Pure types, no React imports.
 *
 * Every keyboard binding in the app is an Action: global navigation, mail
 * verbs, and list/reader motion alike. The command palette, the key
 * dispatcher, the help dialog and the keybindings settings page all read
 * this one table, so a key cannot do one thing while help says another.
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
  | "View"
  | "Move"
  | "Select"
  | "Read";

/** Chord grammar from `lib/keys/chord.ts`, e.g. "g i", "?", "Mod+k". */
export type ShortcutChord = string;

/**
 * Where a binding is live. "global" fires anywhere outside text fields.
 * The others fire while the matching view is mounted and focused: it
 * pushes its scope (`useShortcutScope`) and registers a controller
 * (`useScopeController`) that implements the action's `command`.
 */
export type ActionScope =
  | "global"
  | "sidebar"
  | "list"
  | "reader"
  | "screener"
  | "focus"
  | "place"
  | "todo"
  | "catchup"
  | "expired"
  | "now"
  | "messages"
  | "updates"
  | "archive"
  | "reading"
  | "reading-reader";

export interface ActionContext {
  path: string;
  activePane: MailPane;
  /** Active scopes, innermost first, always ending in "global". */
  scopes: ActionScope[];
  selectionCount: number;
  accountCount: number;
  hasFocusedThread: boolean;
  isFirstAccountOnly: boolean;
}

export type ActionRunner = (ctx: ActionContext) => void | Promise<void>;

export type ActionPredicate = (ctx: ActionContext) => boolean;

type IconComponent = ComponentType<{ className?: string }>;

interface ActionBase {
  id: string;
  label: string;
  /** A few words for the status bar and key hints, when `label` is long. */
  shortLabel?: string;
  description?: string;
  group: ActionGroup;
  icon?: IconComponent;
  shortcut?: ShortcutChord;
  /** More chords bound to the same action in the same scopes. */
  aliases?: ShortcutChord[];
  /**
   * Retired chords that still work for a release so muscle memory has
   * time to move. Bound, but never shown in help, hints or the docs.
   */
  retiredAliases?: ShortcutChord[];
  /** Listed in the palette but never bound to a key. */
  paletteOnly?: boolean;
  /** Scopes the binding is live in; defaults to ["global"]. */
  scopes?: ActionScope[];
  /** Hide from the palette (motion keys are noise there). */
  hideInPalette?: boolean;
  /**
   * Set when the web binding deliberately differs from the TUI's live key
   * (browser conventions, or the TUI key being unreachable). Shown in help.
   */
  tuiNote?: string;
  when?: ActionPredicate;
}

/** A self-contained action: `run` does the work. */
export interface RunAction extends ActionBase {
  run: ActionRunner;
  command?: undefined;
}

/**
 * A scoped action: `command` names a method on the active scope's
 * controller, so "e" archives the focused row in the list and the open
 * thread in the reader.
 */
export interface CommandAction extends ActionBase {
  command: string;
  run?: undefined;
}

export type Action = RunAction | CommandAction;
