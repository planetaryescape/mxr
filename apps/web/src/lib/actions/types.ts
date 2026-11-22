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

export type ShortcutChord = string;

export type ActionScope =
  | "global"
  | "sidebar"
  | "list"
  | "reader"
  | "screener"
  | "focus"
  | "todo"
  | "catchup"
  | "expired"
  | "now"
  | "messages"
  | "messages-filters"
  | "page"
  | "updates"
  | "archive"
  | "reading"
  | "reading-reader"
  | "compose"
  | "arrivals";

export interface ActionContext {
  path: string;
  activePane: MailPane;
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
  shortLabel?: string;
  description?: string;
  group: ActionGroup;
  icon?: IconComponent;
  shortcut?: ShortcutChord;
  aliases?: ShortcutChord[];
  macAliases?: ShortcutChord[];
  retiredAliases?: ShortcutChord[];
  movedAliases?: { chord: ShortcutChord; scopes: ActionScope[]; to?: ShortcutChord }[];
  paletteOnly?: boolean;
  scopes?: ActionScope[];
  hideInPalette?: boolean;
  tuiNote?: string;
  when?: ActionPredicate;
}

export interface RunAction extends ActionBase {
  run: ActionRunner;
  command?: undefined;
}

export interface CommandAction extends ActionBase {
  command: string;
  run?: undefined;
}

export type Action = RunAction | CommandAction;

export type ActionBehavior =
  | Pick<RunAction, "id" | "run" | "command" | "when" | "icon">
  | Pick<CommandAction, "id" | "run" | "command" | "when" | "icon">;
