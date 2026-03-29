import type { Dispatch, SetStateAction } from "react";
import type {
  ComposeFrontmatter,
  ComposeSession,
  ComposeSessionKind,
  FocusContext,
  LayoutMode,
  MailboxRow,
  ReaderMode,
  SearchMode,
  SidebarItem,
  SidebarPayload,
  ThreadResponse,
  WorkbenchScreen,
} from "../../shared/types";
import type { DesktopAction } from "../lib/tui-manifest";
import type { FlattenedEntry } from "../surfaces/types";

type OptimisticRowPatch = {
  unread?: boolean;
  starred?: boolean;
};

type MutationRequestOptions = {
  closeReader?: boolean;
  preserveReader?: boolean;
  optimistic?: OptimisticRowPatch;
  pendingLabel?: string;
};

type SelectionSetters = {
  setMailbox: (threadId: string | null) => void;
  setSearch: (threadId: string | null) => void;
};

export type DesktopActionContext = {
  screen: WorkbenchScreen;
  mailboxRows: FlattenedEntry[];
  searchRows: FlattenedEntry[];
  selectedMailboxThreadId: string | null;
  selectedSearchThreadId: string | null;
  setSelectedMailboxThreadId: (threadId: string | null) => void;
  setSelectedSearchThreadId: (threadId: string | null) => void;
  layoutMode: LayoutMode;
  setLayoutMode: Dispatch<SetStateAction<LayoutMode>>;
  focusContext: FocusContext;
  setFocusContext: Dispatch<SetStateAction<FocusContext>>;
  readerMode: ReaderMode;
  setReaderMode: Dispatch<SetStateAction<ReaderMode>>;
  thread: ThreadResponse | null;
  showInboxZero: boolean;
  setShowInboxZero: Dispatch<SetStateAction<boolean>>;
  helpOpen: boolean;
  setHelpOpen: Dispatch<SetStateAction<boolean>>;
  commandPaletteOpen: boolean;
  setCommandPaletteOpen: Dispatch<SetStateAction<boolean>>;
  setCommandQuery: Dispatch<SetStateAction<string>>;
  setSearchMode: Dispatch<SetStateAction<SearchMode>>;
  closeReader: () => void;
  switchScreen: (next: WorkbenchScreen) => void;
  loadSearch: () => Promise<void>;
  loadRules: () => Promise<void>;
  loadAccounts: () => Promise<void>;
  loadDiagnostics: () => Promise<void>;
  applySidebarLensById: (itemId: string) => Promise<void>;
  applySidebarLens: (item: SidebarItem, options?: { preserveFocus?: boolean }) => Promise<void>;
  archiveSelected: () => Promise<void>;
  mutateSelected: (
    path: string,
    body: Record<string, unknown>,
    options?: MutationRequestOptions,
  ) => Promise<void>;
  effectiveSelection: string[];
  selectedRow: MailboxRow | null;
  openThread: () => void;
  refreshCurrentView: (options?: { preserveReader?: boolean }) => Promise<void>;
  showNotice: (message: string) => void;
  openComposeShell: (kind: ComposeSessionKind, messageId?: string) => Promise<void>;
  openApplyLabelDialog: () => void;
  openMoveDialog: () => void;
  setUnsubscribeDialogOpen: Dispatch<SetStateAction<boolean>>;
  openSnoozeDialog: () => Promise<void>;
  sidebar: SidebarPayload;
  openSelectedInBrowser: () => Promise<void>;
  openAttachmentsPanel: () => void;
  openLinksPanel: () => void;
  signatureExpanded: boolean;
  setSignatureExpanded: Dispatch<SetStateAction<boolean>>;
  remoteContentEnabled: boolean;
  setRemoteContentEnabled: Dispatch<SetStateAction<boolean>>;
  visualMode: boolean;
  setVisualMode: Dispatch<SetStateAction<boolean>>;
  visualAnchorMessageId: string | null;
  setVisualAnchorMessageId: Dispatch<SetStateAction<string | null>>;
  selectedMessageIds: Set<string>;
  setSelectedMessageIds: Dispatch<SetStateAction<Set<string>>>;
  openGoToLabelDialog: () => void;
  openSavedSearchDialog: () => void;
  openMailboxFilter?: () => void;
  setMailListMode: Dispatch<SetStateAction<"threads" | "messages">>;
  exportSelectedThread: () => Promise<void>;
  generateBugReport: () => Promise<void>;
  openDiagnosticsDetails: () => Promise<void>;
  openConfigFile: () => Promise<void>;
  openLogs: () => Promise<void>;
  openRuleForm: (mode: "new" | "edit") => Promise<void>;
  toggleSelectedRuleEnabled: () => Promise<void>;
  openRuleDryRun: () => Promise<void>;
  openRuleHistory: () => Promise<void>;
  deleteSelectedRule: () => Promise<void>;
  openAccountForm: () => void;
  testCurrentAccount: () => Promise<void>;
  makeSelectedAccountDefault: () => Promise<void>;
  formatPendingMutationLabel: (verb: string, count: number, detail?: string) => string;
  switchAccount?: (key: string) => Promise<void>;
  triggerSync: () => Promise<void>;
  composeOpen: boolean;
  composeSession: ComposeSession | null;
  setComposeSession: Dispatch<SetStateAction<ComposeSession | null>>;
  setComposeDraft: Dispatch<SetStateAction<ComposeFrontmatter | null>>;
};

export function runDesktopAction(action: DesktopAction | string, context: DesktopActionContext) {
  switch (action) {
    case "move_down":
    case "scroll_down":
    case "next_message":
      if (context.focusContext === "sidebar") {
        void moveSidebarSelection(1, context.sidebar, context.applySidebarLens);
        return;
      }
      maybeExtendVisualSelection(
        context,
        moveSelection(
        1,
        context.screen,
        context.mailboxRows,
        context.searchRows,
        context.selectedMailboxThreadId,
        context.selectedSearchThreadId,
        {
          setMailbox: context.setSelectedMailboxThreadId,
          setSearch: context.setSelectedSearchThreadId,
        },
        ),
      );
      return;
    case "move_up":
    case "scroll_up":
    case "prev_message":
      if (context.focusContext === "sidebar") {
        void moveSidebarSelection(-1, context.sidebar, context.applySidebarLens);
        return;
      }
      maybeExtendVisualSelection(
        context,
        moveSelection(
        -1,
        context.screen,
        context.mailboxRows,
        context.searchRows,
        context.selectedMailboxThreadId,
        context.selectedSearchThreadId,
        {
          setMailbox: context.setSelectedMailboxThreadId,
          setSearch: context.setSelectedSearchThreadId,
        },
        ),
      );
      return;
    case "page_down":
      if (context.focusContext === "sidebar") {
        void moveSidebarSelection(8, context.sidebar, context.applySidebarLens);
        return;
      }
      maybeExtendVisualSelection(
        context,
        moveSelection(
        8,
        context.screen,
        context.mailboxRows,
        context.searchRows,
        context.selectedMailboxThreadId,
        context.selectedSearchThreadId,
        {
          setMailbox: context.setSelectedMailboxThreadId,
          setSearch: context.setSelectedSearchThreadId,
        },
        ),
      );
      return;
    case "page_up":
      if (context.focusContext === "sidebar") {
        void moveSidebarSelection(-8, context.sidebar, context.applySidebarLens);
        return;
      }
      maybeExtendVisualSelection(
        context,
        moveSelection(
        -8,
        context.screen,
        context.mailboxRows,
        context.searchRows,
        context.selectedMailboxThreadId,
        context.selectedSearchThreadId,
        {
          setMailbox: context.setSelectedMailboxThreadId,
          setSearch: context.setSelectedSearchThreadId,
        },
        ),
      );
      return;
    case "jump_top":
    case "visible_top":
      if (context.focusContext === "sidebar") {
        void jumpSidebarSelection("top", context.sidebar, context.applySidebarLens);
        return;
      }
      maybeExtendVisualSelection(
        context,
        jumpSelection("top", context.screen, context.mailboxRows, context.searchRows, {
        setMailbox: context.setSelectedMailboxThreadId,
        setSearch: context.setSelectedSearchThreadId,
        }),
      );
      return;
    case "jump_bottom":
    case "visible_bottom":
      if (context.focusContext === "sidebar") {
        void jumpSidebarSelection("bottom", context.sidebar, context.applySidebarLens);
        return;
      }
      maybeExtendVisualSelection(
        context,
        jumpSelection("bottom", context.screen, context.mailboxRows, context.searchRows, {
        setMailbox: context.setSelectedMailboxThreadId,
        setSearch: context.setSelectedSearchThreadId,
        }),
      );
      return;
    case "visible_middle":
    case "center_current":
      context.setFocusContext(context.layoutMode === "twoPane" ? "mailList" : "reader");
      return;
    case "next_search_result":
      maybeExtendVisualSelection(
        context,
        moveSelection(
        1,
        context.screen,
        context.mailboxRows,
        context.searchRows,
        context.selectedMailboxThreadId,
        context.selectedSearchThreadId,
        {
          setMailbox: context.setSelectedMailboxThreadId,
          setSearch: context.setSelectedSearchThreadId,
        },
        ),
      );
      return;
    case "prev_search_result":
      maybeExtendVisualSelection(
        context,
        moveSelection(
        -1,
        context.screen,
        context.mailboxRows,
        context.searchRows,
        context.selectedMailboxThreadId,
        context.selectedSearchThreadId,
        {
          setMailbox: context.setSelectedMailboxThreadId,
          setSearch: context.setSelectedSearchThreadId,
        },
        ),
      );
      return;
    case "open":
      // In sidebar: Enter opens the label and moves focus to mail list
      if (context.focusContext === "sidebar") {
        context.setFocusContext("mailList");
        return;
      }
      context.openThread();
      return;
    case "back":
    case "quit_view":
      // Clear stale compose state that could leave the app in a confused state
      if (context.composeSession && !context.composeOpen) {
        context.setComposeSession(null);
        context.setComposeDraft(null);
      }
      if (context.showInboxZero) {
        context.setShowInboxZero(false);
        return;
      }
      if (context.helpOpen) {
        context.setHelpOpen(false);
        return;
      }
      if (context.commandPaletteOpen) {
        context.setCommandPaletteOpen(false);
        context.setCommandQuery("");
        context.setFocusContext(context.screen === "search" ? "search" : "mailList");
        return;
      }
      if (context.layoutMode === "fullScreen") {
        context.setLayoutMode("threePane");
        return;
      }
      if (context.layoutMode === "threePane") {
        context.closeReader();
        return;
      }
      if (context.screen !== "mailbox") {
        context.switchScreen("mailbox");
      }
      return;
    case "search":
    case "open_search_screen":
    case "open_tab_2":
      context.switchScreen("search");
      return;
    case "submit_search":
      context.switchScreen("search");
      void context.loadSearch();
      return;
    case "close_search":
      if (context.screen === "search") {
        context.switchScreen("mailbox");
      }
      return;
    case "cycle_search_mode":
      context.setSearchMode((current) => nextSearchMode(current));
      return;
    case "open_mailbox_screen":
    case "open_tab_1":
      context.switchScreen("mailbox");
      return;
    case "open_rules_screen":
    case "open_tab_3":
      context.switchScreen("rules");
      return;
    case "open_accounts_screen":
    case "open_tab_4":
      context.switchScreen("accounts");
      return;
    case "open_diagnostics_screen":
    case "open_tab_5":
      context.switchScreen("diagnostics");
      return;
    case "go_inbox":
      void context.applySidebarLensById("inbox");
      return;
    case "go_starred":
      void context.applySidebarLensById("starred");
      return;
    case "go_sent":
      void context.applySidebarLensById("sent");
      return;
    case "go_drafts":
      void context.applySidebarLensById("drafts");
      return;
    case "go_all_mail":
      void context.applySidebarLensById("all-mail");
      return;
    case "command_palette":
      context.setCommandPaletteOpen(true);
      context.setFocusContext("commandPalette");
      return;
    case "archive":
      void context.archiveSelected();
      return;
    case "mark_read_archive":
      if (context.effectiveSelection.length === 0) {
        return;
      }
      void context.mutateSelected(
        "/mutations/read-and-archive",
        { message_ids: context.effectiveSelection },
        {
          closeReader: true,
          optimistic: { unread: false },
          pendingLabel: context.formatPendingMutationLabel(
            "Marking",
            context.effectiveSelection.length,
            "read and archiving",
          ),
        },
      );
      return;
    case "trash":
      if (context.effectiveSelection.length === 0) {
        return;
      }
      void context.mutateSelected(
        "/mutations/trash",
        { message_ids: context.effectiveSelection },
        {
          closeReader: true,
          pendingLabel: context.formatPendingMutationLabel(
            "Moving",
            context.effectiveSelection.length,
            "to trash",
          ),
        },
      );
      return;
    case "spam":
      if (context.effectiveSelection.length === 0) {
        return;
      }
      void context.mutateSelected(
        "/mutations/spam",
        { message_ids: context.effectiveSelection },
        {
          closeReader: true,
          pendingLabel: context.formatPendingMutationLabel(
            "Marking",
            context.effectiveSelection.length,
            "as spam",
          ),
        },
      );
      return;
    case "star":
      if (!context.selectedRow || context.effectiveSelection.length === 0) {
        return;
      }
      void context.mutateSelected(
        "/mutations/star",
        { message_ids: context.effectiveSelection, starred: !context.selectedRow.starred },
        {
          preserveReader: true,
          optimistic: { starred: !context.selectedRow.starred },
          pendingLabel: context.formatPendingMutationLabel(
            !context.selectedRow.starred ? "Starring" : "Unstarring",
            context.effectiveSelection.length,
          ),
        },
      );
      return;
    case "mark_read":
      if (context.effectiveSelection.length === 0) {
        return;
      }
      void context.mutateSelected(
        "/mutations/read",
        { message_ids: context.effectiveSelection, read: true },
        {
          preserveReader: true,
          optimistic: { unread: false },
          pendingLabel: context.formatPendingMutationLabel(
            "Marking",
            context.effectiveSelection.length,
            "read",
          ),
        },
      );
      return;
    case "mark_unread":
      if (context.effectiveSelection.length === 0) {
        return;
      }
      void context.mutateSelected(
        "/mutations/read",
        { message_ids: context.effectiveSelection, read: false },
        {
          preserveReader: true,
          optimistic: { unread: true },
          pendingLabel: context.formatPendingMutationLabel(
            "Marking",
            context.effectiveSelection.length,
            "unread",
          ),
        },
      );
      return;
    case "switch_panes":
      context.setFocusContext((current) =>
        nextFocusContext(current, context.layoutMode, context.screen),
      );
      return;
    case "toggle_fullscreen":
      if (!context.thread) {
        return;
      }
      context.setLayoutMode((current) => (current === "fullScreen" ? "threePane" : "fullScreen"));
      return;
    case "toggle_reader_mode":
      context.setReaderMode((current) => nextReaderMode(current));
      return;
    case "toggle_remote_content":
      context.setRemoteContentEnabled((current) => !current);
      return;
    case "help":
      context.setHelpOpen(true);
      return;
    case "refresh_rules":
      void context.loadRules();
      return;
    case "refresh_accounts":
      void context.loadAccounts();
      return;
    case "refresh_diagnostics":
      void context.loadDiagnostics();
      return;
    case "sync":
      void context.triggerSync().then(() => context.refreshCurrentView({ preserveReader: true }));
      context.showNotice("Syncing with server");
      return;
    case "compose":
      void context.openComposeShell("new");
      return;
    case "reply":
      if (context.selectedRow) {
        void context.openComposeShell("reply", context.selectedRow.id);
      }
      return;
    case "reply_all":
      if (context.selectedRow) {
        void context.openComposeShell("reply_all", context.selectedRow.id);
      }
      return;
    case "forward":
      if (context.selectedRow) {
        void context.openComposeShell("forward", context.selectedRow.id);
      }
      return;
    case "apply_label":
      // In sidebar: l opens the active label's mailbox (like TUI)
      if (context.focusContext === "sidebar") {
        context.setFocusContext("mailList");
        return;
      }
      if (context.selectedRow) {
        context.openApplyLabelDialog();
      }
      return;
    case "move_to_label":
      if (context.selectedRow) {
        context.openMoveDialog();
      }
      return;
    case "unsubscribe":
      if (context.selectedRow) {
        context.setUnsubscribeDialogOpen(true);
        context.setFocusContext("dialog");
      }
      return;
    case "snooze":
      if (context.selectedRow) {
        void context.openSnoozeDialog();
      }
      return;
    case "open_subscriptions": {
      const subscription = context.sidebar.sections
        .flatMap((section) => section.items)
        .find((item) => item.lens.kind === "subscription");
      if (subscription) {
        void context.applySidebarLens(subscription);
      }
      return;
    }
    case "open_in_browser":
      void context.openSelectedInBrowser();
      return;
    case "toggle_signature":
      context.setSignatureExpanded((current) => !current);
      return;
    case "attachment_list":
      context.openAttachmentsPanel();
      return;
    case "open_links":
      context.openLinksPanel();
      return;
    case "visual_line_mode":
      context.setVisualMode((current) => !current);
      if (!context.visualMode && context.selectedRow) {
        context.setVisualAnchorMessageId(context.selectedRow.id);
        context.setSelectedMessageIds(new Set([context.selectedRow.id]));
      } else {
        context.setVisualAnchorMessageId(null);
      }
      context.showNotice(!context.visualMode ? "-- VISUAL LINE --" : "Visual mode off");
      return;
    case "export_thread":
      void context.exportSelectedThread();
      return;
    case "toggle_select": {
      if (!context.selectedRow) {
        return;
      }
      let selectionCount = 0;
      context.setSelectedMessageIds((current) => {
        const next = new Set(current);
        if (next.has(context.selectedRow!.id)) {
          next.delete(context.selectedRow!.id);
        } else {
          next.add(context.selectedRow!.id);
        }
        selectionCount = next.size;
        return next;
      });
      context.showNotice(`${selectionCount} selected`);
      return;
    }
    case "clear_selection":
      context.setSelectedMessageIds(new Set());
      context.setVisualMode(false);
      context.setVisualAnchorMessageId(null);
      context.showNotice("Selection cleared");
      return;
    case "select_all": {
      const rows = activeRows(context);
      const ids = new Set(rows.filter((e) => e.kind === "row").map((e) => e.kind === "row" ? e.row.id : ""));
      context.setSelectedMessageIds(ids);
      context.showNotice(`${ids.size} selected`);
      return;
    }
    case "select_none":
      context.setSelectedMessageIds(new Set());
      context.showNotice("Selection cleared");
      return;
    case "select_read": {
      const rows = activeRows(context);
      const ids = new Set(rows.filter((e) => e.kind === "row" && !e.row.unread).map((e) => e.kind === "row" ? e.row.id : ""));
      context.setSelectedMessageIds(ids);
      context.showNotice(`${ids.size} read selected`);
      return;
    }
    case "select_unread": {
      const rows = activeRows(context);
      const ids = new Set(rows.filter((e) => e.kind === "row" && e.row.unread).map((e) => e.kind === "row" ? e.row.id : ""));
      context.setSelectedMessageIds(ids);
      context.showNotice(`${ids.size} unread selected`);
      return;
    }
    case "select_starred": {
      const rows = activeRows(context);
      const ids = new Set(rows.filter((e) => e.kind === "row" && e.row.starred).map((e) => e.kind === "row" ? e.row.id : ""));
      context.setSelectedMessageIds(ids);
      context.showNotice(`${ids.size} starred selected`);
      return;
    }
    case "filter_mailbox":
      context.openMailboxFilter?.();
      return;
    case "go_label":
      context.openGoToLabelDialog();
      return;
    case "create_saved_search":
      context.openSavedSearchDialog();
      return;
    case "toggle_mail_list_mode":
      context.setMailListMode((current) => (current === "threads" ? "messages" : "threads"));
      return;
    case "generate_bug_report":
      void context.generateBugReport();
      return;
    case "open_diagnostics_pane_details":
      void context.openDiagnosticsDetails();
      return;
    case "edit_config":
      void context.openConfigFile();
      return;
    case "open_logs":
      void context.openLogs();
      return;
    case "open_rule_form_new":
      void context.openRuleForm("new");
      return;
    case "open_rule_form_edit":
      void context.openRuleForm("edit");
      return;
    case "toggle_rule_enabled":
      void context.toggleSelectedRuleEnabled();
      return;
    case "show_rule_dry_run":
      void context.openRuleDryRun();
      return;
    case "show_rule_history":
      void context.openRuleHistory();
      return;
    case "delete_rule":
      void context.deleteSelectedRule();
      return;
    case "open_account_form_new":
      context.openAccountForm();
      return;
    case "test_account_form":
      void context.testCurrentAccount();
      return;
    case "set_default_account":
      void context.makeSelectedAccountDefault();
      return;
    default:
      // Dynamic actions like switch_account:key
      if (action.startsWith("switch_account:")) {
        const key = action.slice("switch_account:".length);
        if (key && context.switchAccount) {
          void context.switchAccount(key);
        }
      }
      return;
  }
}

export function nextSearchMode(mode: SearchMode): SearchMode {
  const order: SearchMode[] = ["lexical", "hybrid", "semantic"];
  const index = order.indexOf(mode);
  return order[(index + 1) % order.length] ?? "lexical";
}

export function nextReaderMode(mode: ReaderMode): ReaderMode {
  const order: ReaderMode[] = ["auto", "reader", "html", "raw"];
  const index = order.indexOf(mode);
  return order[(index + 1) % order.length] ?? "auto";
}

export function isTypingTarget(element: Element | null) {
  if (!(element instanceof HTMLElement)) {
    return false;
  }
  return element.tagName === "INPUT" || element.tagName === "TEXTAREA" || element.isContentEditable;
}

export function normalizeKeyToken(event: KeyboardEvent) {
  if (event.metaKey && !event.ctrlKey && event.key.toLowerCase() === "p") {
    return "Ctrl-p";
  }

  if (event.ctrlKey && event.key.length === 1) {
    return `Ctrl-${event.key.toLowerCase()}`;
  }

  if (event.key === "Escape") {
    return "Esc";
  }
  if (event.key === "Enter") {
    return "Enter";
  }
  if (event.key === "Tab") {
    return "Tab";
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    return event.key;
  }
  if (event.key.length === 1) {
    return event.key;
  }
  return null;
}

export function nextFocusContext(
  current: FocusContext,
  layoutMode: LayoutMode,
  screen: WorkbenchScreen,
): FocusContext {
  const order: FocusContext[] =
    layoutMode === "twoPane"
      ? screen === "search"
        ? ["sidebar", "search"]
        : ["sidebar", "mailList"]
      : screen === "search"
        ? ["sidebar", "search", "reader"]
        : ["sidebar", "mailList", "reader"];
  const index = order.indexOf(current);
  return order[(index + 1) % order.length] ?? order[0];
}

function moveSelection(
  delta: number,
  screen: WorkbenchScreen,
  mailboxRows: FlattenedEntry[],
  searchRows: FlattenedEntry[],
  mailboxThreadId: string | null,
  searchThreadId: string | null,
  setters: SelectionSetters,
): Extract<FlattenedEntry, { kind: "row" }>["row"] | null {
  const entries = (screen === "search" ? searchRows : mailboxRows).filter(
    (entry): entry is Extract<FlattenedEntry, { kind: "row" }> => entry.kind === "row",
  );
  const current = screen === "search" ? searchThreadId : mailboxThreadId;
  const currentIndex = entries.findIndex((entry) => entry.row.thread_id === current);
  const nextIndex = clampIndex(currentIndex < 0 ? 0 : currentIndex + delta, entries.length);
  const next = entries[nextIndex]?.row.thread_id ?? null;
  if (screen === "search") {
    setters.setSearch(next);
  } else {
    setters.setMailbox(next);
  }
  return entries[nextIndex]?.row ?? null;
}

function jumpSelection(
  direction: "top" | "bottom",
  screen: WorkbenchScreen,
  mailboxRows: FlattenedEntry[],
  searchRows: FlattenedEntry[],
  setters: SelectionSetters,
): Extract<FlattenedEntry, { kind: "row" }>["row"] | null {
  const entries = (screen === "search" ? searchRows : mailboxRows).filter(
    (entry): entry is Extract<FlattenedEntry, { kind: "row" }> => entry.kind === "row",
  );
  const next =
    direction === "top"
      ? (entries[0]?.row.thread_id ?? null)
      : (entries[entries.length - 1]?.row.thread_id ?? null);
  if (screen === "search") {
    setters.setSearch(next);
  } else {
    setters.setMailbox(next);
  }
  return direction === "top" ? (entries[0]?.row ?? null) : (entries[entries.length - 1]?.row ?? null);
}

function maybeExtendVisualSelection(
  context: Pick<
    DesktopActionContext,
    | "screen"
    | "mailboxRows"
    | "searchRows"
    | "visualMode"
    | "visualAnchorMessageId"
    | "selectedRow"
    | "setSelectedMessageIds"
    | "setVisualAnchorMessageId"
  >,
  nextRow: MailboxRow | null,
) {
  if (!context.visualMode || !nextRow) {
    return;
  }

  const entries = (context.screen === "search" ? context.searchRows : context.mailboxRows).filter(
    (entry): entry is Extract<FlattenedEntry, { kind: "row" }> => entry.kind === "row",
  );
  const anchorId = context.visualAnchorMessageId ?? context.selectedRow?.id ?? nextRow.id;
  const anchorIndex = entries.findIndex((entry) => entry.row.id === anchorId);
  const targetIndex = entries.findIndex((entry) => entry.row.id === nextRow.id);

  context.setVisualAnchorMessageId(anchorId);
  if (anchorIndex < 0 || targetIndex < 0) {
    context.setSelectedMessageIds(new Set([nextRow.id]));
    return;
  }

  const [start, end] = anchorIndex < targetIndex ? [anchorIndex, targetIndex] : [targetIndex, anchorIndex];
  context.setSelectedMessageIds(new Set(entries.slice(start, end + 1).map((entry) => entry.row.id)));
}

function clampIndex(index: number, length: number) {
  if (length === 0) {
    return 0;
  }
  return Math.min(Math.max(index, 0), length - 1);
}

async function moveSidebarSelection(
  delta: number,
  sidebar: SidebarPayload,
  applySidebarLens: (item: SidebarItem, options?: { preserveFocus?: boolean }) => Promise<void>,
) {
  const items = sidebar.sections.flatMap((section) => section.items);
  if (items.length === 0) {
    return;
  }
  const currentIndex = items.findIndex((item) => item.active);
  const nextIndex = clampIndex(currentIndex < 0 ? 0 : currentIndex + delta, items.length);
  const next = items[nextIndex];
  if (next) {
    await applySidebarLens(next, { preserveFocus: true });
  }
}

async function jumpSidebarSelection(
  direction: "top" | "bottom",
  sidebar: SidebarPayload,
  applySidebarLens: (item: SidebarItem, options?: { preserveFocus?: boolean }) => Promise<void>,
) {
  const items = sidebar.sections.flatMap((section) => section.items);
  if (items.length === 0) {
    return;
  }
  const next = direction === "top" ? items[0] : items[items.length - 1];
  if (next) {
    await applySidebarLens(next, { preserveFocus: true });
  }
}

function activeRows(context: DesktopActionContext): FlattenedEntry[] {
  return context.screen === "search" ? context.searchRows : context.mailboxRows;
}
