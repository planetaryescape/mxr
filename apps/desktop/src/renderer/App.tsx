import { Database, Inbox, RefreshCw, ScanSearch, Sparkles, UserRoundCog } from "lucide-react";
import {
  startTransition,
  useDeferredValue,
  useEffect,
  useEffectEvent,
  useMemo,
  useRef,
  useState,
} from "react";
import type { RefObject, SetStateAction } from "react";
import type {
  ActionAckResponse,
  AccountOperationResponse,
  AccountsResponse,
  BridgeState,
  ComposeFrontmatter,
  ComposeSession,
  DiagnosticsResponse,
  FocusContext,
  LayoutMode,
  MailboxGroup,
  MailboxPayload,
  MailboxRow,
  ReaderMode,
  RulesResponse,
  SearchResponse,
  SearchMode,
  SearchScope,
  SearchSort,
  SnoozePreset,
  SidebarItem,
  SidebarLens,
  SidebarPayload,
  ThreadResponse,
  UtilityRailPayload,
  WorkbenchScreen,
  WorkbenchShellPayload,
  RuleFormPayload,
} from "../shared/types";
import {
  bindingsForContext,
  commandPaletteEntries,
  type DesktopAction,
  type DesktopBindingContext,
} from "./lib/tui-manifest";
import { runDesktopAction } from "./state/desktop-actions";
import { fetchJson } from "./state/bridgeHttp";
import { useDesktopAppState } from "./state/useDesktopAppState";
import { useComposeActions } from "./state/useComposeActions";
import { useDesktopKeyboardShortcuts } from "./state/useDesktopKeyboardShortcuts";
import { useMailboxDialogActions } from "./state/useMailboxDialogActions";
import { useRulesAccountsActions } from "./state/useRulesAccountsActions";
import { useWorkbenchShellActions } from "./state/useWorkbenchShellActions";
import { useWorkbenchCoreState } from "./state/useWorkbenchCoreState";
import type { ConnectionStatus } from "./state/useEventStream";
import { useContextMenu, ContextMenuOverlay, type ContextMenuItem } from "./lib/context-menu";
import { DesktopDialogs } from "./surfaces/DesktopDialogs";
import {
  BridgeErrorView,
  BridgeLoadingView,
  BridgeMismatchView,
  CommandPaletteOverlay,
  HelpOverlay,
  InboxZeroOverlay,
} from "./surfaces/Overlays";
import type { FlattenedEntry } from "./surfaces/types";
import { WorkbenchContent } from "./surfaces/WorkbenchContent";
import {
  NavigationSidebar,
  WorkbenchHeader,
  WorkbenchStatusBar,
} from "./surfaces/WorkbenchChrome";

const UPDATE_STEPS = [
  "Homebrew: brew upgrade mxr",
  "Release install: rerun ./install.sh",
  "Source install: git pull && cargo install --path crates/daemon --locked",
];

const EMPTY_SHELL: WorkbenchShellPayload = {
  accountLabel: "personal",
  syncLabel: "Starting",
  statusMessage: "Booting local workspace",
  commandHint: "Ctrl-P",
};

const EMPTY_MAILBOX: MailboxPayload = {
  lensLabel: "Inbox",
  counts: { unread: 0, total: 0 },
  groups: [],
};

const EMPTY_SEARCH: SearchResponse = {
  scope: "threads",
  sort: "relevant",
  mode: "lexical",
  total: 0,
  groups: [],
  explain: null,
};

const EMPTY_SIDEBAR: SidebarPayload = {
  sections: [],
};

const PREVIEW_MARK_READ_DELAY_MS = 5_000;

const SCREEN_ORDER: Array<{
  id: WorkbenchScreen;
  label: string;
  icon: typeof Inbox;
  accent: string;
}> = [
  { id: "mailbox", label: "Mailbox", icon: Inbox, accent: "text-accent" },
  { id: "search", label: "Search", icon: ScanSearch, accent: "text-warning" },
  { id: "rules", label: "Rules", icon: Sparkles, accent: "text-success" },
  { id: "accounts", label: "Accounts", icon: UserRoundCog, accent: "text-foreground" },
  { id: "diagnostics", label: "Diagnostics", icon: Database, accent: "text-danger" },
];

type PendingPreviewReadState = {
  messageId: string;
  timeoutId: number;
};

type OptimisticRowPatch = {
  unread?: boolean;
  starred?: boolean;
};

const EMPTY_MESSAGE_ID_SET = new Set<string>();

export default function App() {
  const {
    bridge,
    setBridge,
    externalPath,
    setExternalPath,
    screen,
    setScreen,
    layoutMode,
    setLayoutMode,
    focusContext,
    setFocusContext,
    readerMode,
    setReaderMode,
    shell,
    setShell,
    sidebar,
    setSidebar,
    mailbox,
    setMailbox,
    searchState,
    setSearchState,
    selectedMailboxThreadId,
    setSelectedMailboxThreadId,
    selectedSearchThreadId,
    setSelectedSearchThreadId,
    thread,
    setThread,
    rulesState,
    setRulesState,
    accountsState,
    setAccountsState,
    diagnosticsState,
    setDiagnosticsState,
  } = useWorkbenchCoreState({
    emptyShell: EMPTY_SHELL,
    emptySidebar: EMPTY_SIDEBAR,
    emptyMailbox: EMPTY_MAILBOX,
    emptySearch: EMPTY_SEARCH,
  });
  const {
    searchQuery,
    setSearchQuery,
    searchScope,
    setSearchScope,
    searchMode,
    setSearchMode,
    searchSort,
    setSearchSort,
    searchExplain,
    setSearchExplain,
    pendingBinding,
    setPendingBinding,
    commandPaletteOpen,
    setCommandPaletteOpen,
    commandQuery,
    setCommandQuery,
    helpOpen,
    setHelpOpen,
    actionNotice,
    setActionNotice,
    pendingMutation,
    setPendingMutation,
    showInboxZero,
    setShowInboxZero,
    workbenchReady,
    setWorkbenchReady,
    mailListMode,
    setMailListMode,
    signatureExpanded,
    setSignatureExpanded,
    remoteContentEnabled,
    setRemoteContentEnabled,
    selectedMessageIds,
    setSelectedMessageIds,
    visualMode,
    setVisualMode,
    visualAnchorMessageId,
    setVisualAnchorMessageId,
    composeSession,
    setComposeSession,
    composeOpen,
    setComposeOpen,
    composeDraft,
    setComposeDraft,
    composeBusy,
    setComposeBusy,
    composeError,
    setComposeError,
    labelDialogOpen,
    setLabelDialogOpen,
    selectedLabels,
    setSelectedLabels,
    customLabel,
    setCustomLabel,
    moveDialogOpen,
    setMoveDialogOpen,
    moveTargetLabel,
    setMoveTargetLabel,
    snoozeDialogOpen,
    setSnoozeDialogOpen,
    snoozePresets,
    setSnoozePresets,
    selectedSnooze,
    setSelectedSnooze,
    unsubscribeDialogOpen,
    setUnsubscribeDialogOpen,
    goToLabelOpen,
    setGoToLabelOpen,
    jumpTargetLabel,
    setJumpTargetLabel,
    savedSearchDialogOpen,
    setSavedSearchDialogOpen,
    savedSearchName,
    setSavedSearchName,
    attachmentDialogOpen,
    setAttachmentDialogOpen,
    linksDialogOpen,
    setLinksDialogOpen,
    reportOpen,
    setReportOpen,
    reportTitle,
    setReportTitle,
    reportContent,
    setReportContent,
    selectedRuleId,
    setSelectedRuleId,
    ruleDetail,
    setRuleDetail,
    rulePanelMode,
    setRulePanelMode,
    ruleHistoryState,
    setRuleHistoryState,
    ruleDryRunState,
    setRuleDryRunState,
    ruleStatus,
    setRuleStatus,
    ruleFormOpen,
    setRuleFormOpen,
    ruleFormBusy,
    setRuleFormBusy,
    ruleFormState,
    setRuleFormState,
    selectedAccountId,
    setSelectedAccountId,
    accountStatus,
    setAccountStatus,
    accountResult,
    setAccountResult,
    accountFormOpen,
    setAccountFormOpen,
    accountFormBusy,
    setAccountFormBusy,
    accountDraftJson,
    setAccountDraftJson,
    modalOpen,
    closeAllDialogs,
  } = useDesktopAppState();

  const deferredSearchQuery = useDeferredValue(searchQuery);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const commandInputRef = useRef<HTMLInputElement | null>(null);
  const isMacPlatform = useMemo(() => {
    if (typeof navigator === "undefined") {
      return false;
    }
    return /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent);
  }, []);

  const mailboxRows = useMemo(() => flattenGroups(mailbox.groups), [mailbox.groups]);
  const searchRows = useMemo(() => flattenGroups(searchState.groups), [searchState.groups]);
  const selectedMailboxRow = useMemo(
    () => findRowByThreadId(mailbox.groups, selectedMailboxThreadId),
    [mailbox.groups, selectedMailboxThreadId],
  );
  const selectedSearchRow = useMemo(
    () => findRowByThreadId(searchState.groups, selectedSearchThreadId),
    [searchState.groups, selectedSearchThreadId],
  );
  const selectedRow = screen === "search" ? selectedSearchRow : selectedMailboxRow;
  const currentThreadId = screen === "search" ? selectedSearchThreadId : selectedMailboxThreadId;
  const effectiveReaderMode = resolveReaderMode(readerMode, thread);
  const platformShell = useMemo(
    () => ({
      ...shell,
      commandHint: isMacPlatform ? "⌘P" : "Ctrl-P",
    }),
    [isMacPlatform, shell],
  );
  const utilityRail = thread?.right_rail ?? defaultUtilityRail(platformShell, selectedRow);
  const activeSidebarItem = useMemo(() => findActiveSidebarItem(sidebar), [sidebar]);
  const labelOptions = useMemo(() => collectLabelOptions(sidebar), [sidebar]);
  const jumpLabelOptions = useMemo(() => collectJumpTargets(sidebar), [sidebar]);
  const threadLinks = useMemo(() => collectLinks(thread), [thread]);
  const threadAttachments = useMemo(() => collectAttachments(thread), [thread]);
  const selectedRule = useMemo(
    () =>
      rulesState.rules.find((rule) => String(rule.id ?? rule.name ?? "") === selectedRuleId) ??
      null,
    [rulesState.rules, selectedRuleId],
  );
  const selectedAccount = useMemo(
    () =>
      accountsState.accounts.find((account) => account.account_id === selectedAccountId) ?? null,
    [accountsState.accounts, selectedAccountId],
  );
  const effectiveSelection = useMemo(
    () =>
      selectedMessageIds.size > 0 ? [...selectedMessageIds] : selectedRow ? [selectedRow.id] : [],
    [selectedMessageIds, selectedRow],
  );
  const pendingMessageIds = useMemo(
    () => pendingMutation?.messageIds ?? EMPTY_MESSAGE_ID_SET,
    [pendingMutation],
  );
  const searchRefreshKey = `${deferredSearchQuery}\u0000${searchScope}\u0000${searchMode}\u0000${searchSort}\u0000${searchExplain ? "1" : "0"}`;
  const bindingContext: DesktopBindingContext =
    layoutMode === "twoPane" ? "mailList" : "threadView";
  const helpSections = useMemo(
    () => [
      {
        id: "mailList",
        title: "Mail list",
        entries: bindingsForContext("mailList").map((entry) => ({
          ...entry,
          display: displayShortcut(entry.action, entry.display, isMacPlatform),
        })),
      },
      {
        id: "threadView",
        title: "Thread view",
        entries: bindingsForContext("threadView").map((entry) => ({
          ...entry,
          display: displayShortcut(entry.action, entry.display, isMacPlatform),
        })),
      },
      {
        id: "messageView",
        title: "Message view",
        entries: bindingsForContext("messageView").map((entry) => ({
          ...entry,
          display: displayShortcut(entry.action, entry.display, isMacPlatform),
        })),
      },
    ],
    [isMacPlatform],
  );

  const [mailboxFilterOpen, setMailboxFilterOpen] = useState(false);
  const [mailboxFilterQuery, setMailboxFilterQuery] = useState("");
  const contextMenu = useContextMenu();

  useActionNoticeTimeout(actionNotice, setActionNotice);
  usePruneSelectedMessages(mailbox.groups, searchState.groups, setSelectedMessageIds);

  useEffect(() => {
    document.body.setAttribute("data-remote-content", String(remoteContentEnabled));
  }, [remoteContentEnabled]);

  const showNotice = useEffectEvent((message: string) => {
    setActionNotice(message);
  });

  const commandActions = useMemo(
    () => [
      ...commandPaletteEntries().map((item) => ({
        ...item,
        shortcut: displayShortcut(item.action, item.shortcut, isMacPlatform),
      })),
      { action: "filter_mailbox", category: "Navigation", label: "Filter mailbox", shortcut: "Ctrl-F" },
      { action: "select_all", category: "Selection", label: "Select all", shortcut: "" },
      { action: "select_none", category: "Selection", label: "Select none", shortcut: "" },
      { action: "select_read", category: "Selection", label: "Select read", shortcut: "" },
      { action: "select_unread", category: "Selection", label: "Select unread", shortcut: "" },
      { action: "select_starred", category: "Selection", label: "Select starred", shortcut: "" },
      { action: "create_saved_search", category: "Search", label: "Save current search", shortcut: "" },
      { action: "toggle_remote_content", category: "View", label: "Toggle remote content", shortcut: "M" },
      ...accountsState.accounts.map((a) => ({
        action: `switch_account:${a.key ?? a.account_id}`,
        category: "Account",
        label: `Switch to ${a.name}`,
        shortcut: "",
      })),
    ],
    [isMacPlatform, accountsState.accounts],
  );

  const {
    loadMailbox,
    loadSearch,
    loadMoreSearch,
    loadThread,
    loadRules,
    loadAccounts,
    loadDiagnostics,
    openThread,
    closeReader,
    refreshBridge,
    applySidebarLens,
    applySidebarLensById,
    switchScreen,
  } = useWorkbenchShellActions({
    bridge,
    deferredSearchQuery,
    searchScope,
    searchMode,
    searchSort,
    searchExplain,
    sidebar,
    selectedRow,
    screen,
    setBridge,
    setShell,
    setSidebar,
    setMailbox,
    setScreen,
    setLayoutMode,
    setThread,
    setSelectedMailboxThreadId,
    setSelectedSearchThreadId,
    setShowInboxZero,
    setWorkbenchReady,
    searchState,
    setSearchState,
    setRulesState,
    setSelectedRuleId,
    setAccountsState,
    setSelectedAccountId,
    setDiagnosticsState,
    setFocusContext,
    setCommandPaletteOpen,
    setCommandQuery,
    setReaderMode,
    setSignatureExpanded,
    searchInputRef,
  });

  const {
    refreshCurrentView,
    runPendingMutation,
    applyOptimisticRowPatch,
    mutateSelected,
    archiveSelected,
  } = useMailboxMutationActions({
    screen,
    currentThreadId,
    layoutMode,
    bridge,
    activeSidebarItem,
    mailbox,
    searchState,
    thread,
    effectiveSelection,
    selectedRow,
    setPendingMutation,
    setMailbox,
    setSearchState,
    setThread,
    loadSearch,
    loadThread,
    loadMailbox,
    closeReader,
    showNotice,
  });

  const {
    persistComposeDraft,
    refreshComposeSession,
    openComposeShell,
    closeComposeShell,
    submitComposeAction,
    discardComposeSession,
    launchComposeEditor,
    setComposeBody,
  } = useComposeActions({
    bridge,
    composeSession,
    composeDraft,
    composeOpen,
    screen,
    setComposeSession,
    setComposeDraft,
    setComposeError,
    setComposeBusy,
    setComposeOpen,
    setFocusContext,
    showNotice,
    refreshCurrentView,
  });

  const syncComposeDraft = useEffectEvent(async () => {
    try {
      await persistComposeDraft();
    } catch (error) {
      setComposeError(error instanceof Error ? error.message : "Failed to update draft");
    }
  });

  useEffect(() => {
    if (!composeSession || !composeDraft || bridge.kind !== "ready") {
      return;
    }

    const timeout = window.setTimeout(() => {
      void syncComposeDraft();
    }, 220);

    return () => window.clearTimeout(timeout);
  }, [bridge.kind, composeSession, composeDraft]);

  useComposeWindowRefresh(composeOpen, composeSession, refreshComposeSession);

  const {
    loadSelectedRuleDetail,
    openRuleHistory,
    openRuleDryRun,
    openRuleForm,
    saveRuleForm,
    toggleSelectedRuleEnabled,
    deleteSelectedRule,
    openAccountForm,
    testCurrentAccount,
    saveAccountDraft,
    makeSelectedAccountDefault,
  } = useRulesAccountsActions({
    bridge,
    selectedRuleId,
    selectedRule,
    selectedAccount,
    ruleFormState,
    accountDraftJson,
    accountFormOpen,
    setFocusContext,
    setRuleDetail,
    setRulePanelMode,
    setRuleHistoryState,
    setRuleDryRunState,
    setRuleStatus,
    setRuleFormOpen,
    setRuleFormBusy,
    setRuleFormState,
    setSelectedRuleId,
    setAccountStatus,
    setAccountResult,
    setAccountFormOpen,
    setAccountFormBusy,
    setAccountDraftJson,
    loadRules,
    loadAccounts,
    showNotice,
  });

  useWorkbenchLifecycle({
    bridge,
    screen,
    searchRefreshKey,
    layoutMode,
    currentThreadId,
    selectedRow,
    mailbox,
    searchState,
    thread,
    selectedRuleId,
    commandPaletteOpen,
    commandInputRef,
    setBridge,
    loadMailbox,
    loadSearch,
    loadThread,
    loadRules,
    loadAccounts,
    loadDiagnostics,
    loadSelectedRuleDetail,
    applyOptimisticRowPatch,
    runPendingMutation,
    refreshCurrentView,
    setMailbox,
    setSearchState,
    setThread,
    showNotice,
  });

  const {
    openApplyLabelDialog,
    applyLabels,
    openMoveDialog,
    moveSelectedMessage,
    openSnoozeDialog,
    snoozeSelectedMessage,
    confirmUnsubscribe,
    openExternalUrl,
    openSelectedInBrowser,
    openLinksPanel,
    openAttachmentsPanel,
    runAttachmentAction,
    openGoToLabelDialog,
    applyJumpTarget,
    exportSelectedThread,
    generateBugReport,
  } = useMailboxDialogActions({
    bridge,
    screen,
    layoutMode,
    selectedRow,
    effectiveSelection,
    labelOptions,
    selectedLabels,
    customLabel,
    moveTargetLabel,
    selectedSnooze,
    jumpLabelOptions,
    jumpTargetLabel,
    threadLinks,
    threadAttachments,
    setFocusContext,
    setSelectedLabels,
    setCustomLabel,
    setLabelDialogOpen,
    setMoveTargetLabel,
    setMoveDialogOpen,
    setSnoozePresets,
    setSelectedSnooze,
    setSnoozeDialogOpen,
    setUnsubscribeDialogOpen,
    setJumpTargetLabel,
    setGoToLabelOpen,
    setAttachmentDialogOpen,
    setLinksDialogOpen,
    setReportTitle,
    setReportContent,
    setReportOpen,
    showNotice,
    runPendingMutation,
    refreshCurrentView,
    closeReader,
    applySidebarLens,
    formatPendingMutationLabel,
  });

  const ensureDiagnosticsReport = useEffectEvent(async () => {
    if (diagnosticsState) {
      return diagnosticsState.report;
    }
    if (bridge.kind !== "ready") {
      return null;
    }
    const payload = await fetchJson<DiagnosticsResponse>(
      bridge.baseUrl,
      bridge.authToken,
      "/diagnostics",
    );
    setDiagnosticsState(payload);
    return payload.report;
  });

  const openDiagnosticsDetails = useEffectEvent(async () => {
    try {
      switchScreen("diagnostics");
      const report = await ensureDiagnosticsReport();
      if (!report) {
        return;
      }
      setReportTitle("Diagnostics details");
      setReportContent(formatDiagnosticsDetails(report));
      setReportOpen(true);
      setFocusContext("dialog");
    } catch (error) {
      showNotice(error instanceof Error ? error.message : "Failed to open diagnostics details");
    }
  });

  const openConfigFile = useEffectEvent(async () => {
    try {
      await window.mxrDesktop.openConfigFile();
      showNotice("Opened config file");
    } catch (error) {
      showNotice(error instanceof Error ? error.message : "Failed to open config file");
    }
  });

  const openLogs = useEffectEvent(async () => {
    try {
      const report = await ensureDiagnosticsReport();
      if (!report?.log_path) {
        showNotice("No log file path available");
        return;
      }
      await window.mxrDesktop.openLocalPath(report.log_path);
      showNotice("Opened log file");
    } catch (error) {
      showNotice(error instanceof Error ? error.message : "Failed to open log file");
    }
  });

  const filteredCommands = useMemo(() => {
    if (!commandQuery) return commandActions;
    const query = commandQuery.toLowerCase();
    return commandActions.filter(
      (item) =>
        item.label.toLowerCase().includes(query) ||
        item.shortcut.toLowerCase().includes(query) ||
        item.category.toLowerCase().includes(query) ||
      item.action.toLowerCase().includes(query),
    );
  }, [commandActions, commandQuery]);

  const [selectedCommandIndex, setSelectedCommandIndex] = useState(0);

  useEffect(() => {
    if (commandPaletteOpen) {
      setSelectedCommandIndex(0);
    }
  }, [commandPaletteOpen, commandQuery]);

  useEffect(() => {
    if (!commandPaletteOpen || filteredCommands.length === 0) {
      setSelectedCommandIndex(0);
      return;
    }
    setSelectedCommandIndex((current) => Math.min(current, filteredCommands.length - 1));
  }, [commandPaletteOpen, filteredCommands.length]);

  const runSelectedCommand = useEffectEvent(() => {
    const command = filteredCommands[selectedCommandIndex];
    if (!command) {
      return;
    }
    dispatchAction(command.action);
    setCommandPaletteOpen(false);
    setCommandQuery("");
  });

  const dispatchAction = useDesktopActionDispatcher({
    screen,
    mailboxRows,
    searchRows,
    selectedMailboxThreadId,
    selectedSearchThreadId,
    setSelectedMailboxThreadId,
    setSelectedSearchThreadId,
    layoutMode,
    setLayoutMode,
    focusContext,
    setFocusContext,
    readerMode,
    setReaderMode,
    thread,
    showInboxZero,
    setShowInboxZero,
    helpOpen,
    setHelpOpen,
    commandPaletteOpen,
    setCommandPaletteOpen,
    setCommandQuery,
    setSearchMode,
    closeReader,
    switchScreen,
    loadSearch,
    loadRules,
    loadAccounts,
    loadDiagnostics,
    applySidebarLensById,
    applySidebarLens,
    archiveSelected,
    mutateSelected,
    effectiveSelection,
    selectedRow,
    openThread,
    refreshCurrentView,
    showNotice,
    openComposeShell,
    openApplyLabelDialog,
    openMoveDialog,
    setUnsubscribeDialogOpen,
    openSnoozeDialog,
    sidebar,
    openSelectedInBrowser,
    openAttachmentsPanel,
    openLinksPanel,
    signatureExpanded,
    setSignatureExpanded,
    remoteContentEnabled,
    setRemoteContentEnabled,
    visualMode,
    setVisualMode,
    visualAnchorMessageId,
    setVisualAnchorMessageId,
    selectedMessageIds,
    setSelectedMessageIds,
    openGoToLabelDialog,
    openSavedSearchDialog: () => {
      setSavedSearchName("");
      setSavedSearchDialogOpen(true);
      setFocusContext("dialog");
    },
    openMailboxFilter: () => {
      setMailboxFilterOpen(true);
    },
    setMailListMode,
    exportSelectedThread,
    generateBugReport,
    openDiagnosticsDetails,
    openConfigFile,
    openLogs,
    openRuleForm,
    toggleSelectedRuleEnabled,
    openRuleDryRun,
    openRuleHistory,
    deleteSelectedRule,
    openAccountForm,
    testCurrentAccount,
    makeSelectedAccountDefault,
    formatPendingMutationLabel,
    triggerSync: async () => {
      if (bridge.kind !== "ready") return;
      await fetchJson(bridge.baseUrl, bridge.authToken, "/sync", { method: "POST" });
    },
    switchAccount: async (key: string) => {
      if (bridge.kind !== "ready") return;
      await fetchJson(bridge.baseUrl, bridge.authToken, "/accounts/default", {
        method: "POST",
        body: JSON.stringify({ key }),
      });
      await refreshCurrentView({ preserveReader: false });
    },
    composeOpen,
    composeSession,
    setComposeSession,
    setComposeDraft,
  });

  useDesktopKeyboardShortcuts({
    bindingContext,
    pendingBinding,
    setPendingBinding,
    commandPaletteOpen,
    filteredCommandCount: filteredCommands.length,
    selectedCommandIndex,
    setSelectedCommandIndex,
    runSelectedCommand,
    screen,
    focusContext,
    modalOpen,
    composeOpen,
    closeComposeShell,
    submitCompose: (action) => {
      if (action === "send") void submitComposeAction("/compose/session/send", "Sent");
      else if (action === "save") void submitComposeAction("/compose/session/save", "Draft saved");
    },
    closeAllDialogs,
    setFocusContext,
    selectedMessageIds,
    visualMode,
    dispatchAction,
  });

  // Event stream status is managed by EventStreamBridge in main.tsx (Electron context only).
  // In test environments, this stays "disconnected".
  const eventStreamStatus: ConnectionStatus = "disconnected";

  const bridgeGate = renderBridgeGate({
    bridge,
    workbenchReady,
    externalPath,
    setExternalPath,
    setBridge,
    refreshBridge,
  });
  if (bridgeGate) {
    return bridgeGate;
  }
  const readyBridge = bridge as Extract<BridgeState, { kind: "ready" }>;
  return renderDesktopWorkbench({
    screen,
    shell: platformShell,
    sidebar,
    searchQuery,
    setSearchQuery,
    switchScreen,
    applySidebarLens,
    pendingBinding,
    actionNotice,
    pendingMutation,
    composeSession,
    composeOpen,
    setComposeOpen,
    setFocusContext,
    selectedRow,
    openComposeShell,
    openApplyLabelDialog,
    openSnoozeDialog,
    refreshCurrentView,
    mailboxFilterOpen,
    mailboxFilterQuery,
    setMailboxFilterOpen,
    setMailboxFilterQuery,
    mailbox,
    mailboxRows,
    mailListMode,
    selectedMailboxThreadId,
    selectedMessageIds,
    pendingMessageIds,
    setSelectedMailboxThreadId,
    openThread,
    layoutMode,
    thread,
    effectiveReaderMode,
    setReaderMode,
    signatureExpanded,
    archiveSelected,
    closeReader,
    utilityRail,
    searchInputRef,
    searchScope,
    setSearchScope,
    searchMode,
    setSearchMode,
    searchSort,
    setSearchSort,
    searchExplain,
    setSearchExplain,
    searchState,
    searchRows,
    loadMoreSearch: () => loadMoreSearch(),
    selectedSearchThreadId,
    setSelectedSearchThreadId,
    rulesState,
    selectedRuleId,
    rulePanelMode,
    ruleDetail,
    ruleHistoryState,
    ruleDryRunState,
    ruleStatus,
    setSelectedRuleId,
    openRuleForm,
    toggleSelectedRuleEnabled,
    openRuleHistory,
    openRuleDryRun,
    deleteSelectedRule,
    accountsState,
    selectedAccountId,
    accountStatus,
    accountResult,
    setSelectedAccountId,
    openAccountForm,
    testCurrentAccount,
    makeSelectedAccountDefault,
    readyBridge,
    diagnosticsState,
    generateBugReport,
    focusContext,
    commandPaletteOpen,
    commandInputRef,
    commandQuery,
    setCommandQuery,
    filteredCommands,
    selectedCommandIndex,
    setSelectedCommandIndex,
    runSelectedCommand,
    dispatchAction,
    setCommandPaletteOpen,
    helpOpen,
    helpSections,
    setHelpOpen,
    showInboxZero,
    setShowInboxZero,
    composeDraft,
    composeBusy,
    composeError,
    setComposeDraft,
    closeComposeShell,
    launchComposeEditor,
    refreshComposeSession,
    submitComposeAction,
    persistComposeDraft,
    discardComposeSession,
    setComposeBody,
    labelDialogOpen,
    labelOptions,
    selectedLabels,
    customLabel,
    setLabelDialogOpen,
    setSelectedLabels,
    setCustomLabel,
    applyLabels,
    moveDialogOpen,
    moveTargetLabel,
    setMoveDialogOpen,
    setMoveTargetLabel,
    moveSelectedMessage,
    snoozeDialogOpen,
    snoozePresets,
    selectedSnooze,
    setSnoozeDialogOpen,
    setSelectedSnooze,
    snoozeSelectedMessage,
    unsubscribeDialogOpen,
    setUnsubscribeDialogOpen,
    confirmUnsubscribe,
    goToLabelOpen,
    jumpLabelOptions,
    jumpTargetLabel,
    setGoToLabelOpen,
    setJumpTargetLabel,
    applyJumpTarget,
    savedSearchDialogOpen,
    savedSearchName,
    setSavedSearchDialogOpen,
    setSavedSearchName,
    submitSavedSearch: async () => {
      if (bridge.kind !== "ready" || !savedSearchName.trim()) return;
      await fetchJson(bridge.baseUrl, bridge.authToken, "/saved-searches/create", {
        method: "POST",
        body: JSON.stringify({
          name: savedSearchName.trim(),
          query: searchQuery,
          search_mode: searchMode,
        }),
      });
      setSavedSearchDialogOpen(false);
      setSavedSearchName("");
      setFocusContext("sidebar");
      await refreshCurrentView();
    },
    attachmentDialogOpen,
    threadAttachments,
    setAttachmentDialogOpen,
    runAttachmentAction,
    linksDialogOpen,
    threadLinks,
    setLinksDialogOpen,
    openExternalUrl,
    reportOpen,
    reportTitle,
    reportContent,
    setReportOpen,
    ruleFormOpen,
    ruleFormBusy,
    ruleFormState,
    setRuleFormOpen,
    setRuleFormState,
    saveRuleForm,
    accountFormOpen,
    accountFormBusy,
    accountDraftJson,
    setAccountFormOpen,
    setAccountDraftJson,
    saveAccountDraft,
    eventStreamStatus,
    contextMenu,
  });
}

function flattenGroups(groups: MailboxGroup[]): FlattenedEntry[] {
  return groups.flatMap((group) => [
    { kind: "header" as const, id: `header-${group.id}`, label: group.label },
    ...group.rows.map((row) => ({ kind: "row" as const, id: row.id, row })),
  ]);
}

function buildKnownSenders(groups: MailboxGroup[]): Array<{ name: string; email: string }> {
  const seen = new Map<string, { name: string; email: string }>();
  for (const group of groups) {
    for (const row of group.rows) {
      const email = row.sender_detail ?? row.sender;
      if (!seen.has(email)) {
        seen.set(email, { name: row.sender, email });
      }
    }
  }
  const senders = Array.from(seen.values());
  senders.sort((a, b) => a.email.toLowerCase().localeCompare(b.email.toLowerCase()));
  return senders;
}

function displayShortcut(action: string, display: string, isMacPlatform: boolean) {
  if (isMacPlatform && action === "command_palette" && display === "Ctrl-p") {
    return "⌘P";
  }
  return display;
}

type StateSetter<T> = (updater: SetStateAction<T>) => void;

function renderDesktopWorkbench(props: {
  screen: WorkbenchScreen;
  shell: WorkbenchShellPayload;
  sidebar: SidebarPayload;
  searchQuery: string;
  setSearchQuery: StateSetter<string>;
  switchScreen: (next: WorkbenchScreen) => void;
  applySidebarLens: (item: SidebarItem) => Promise<void>;
  pendingBinding: { tokens: string[] } | null;
  actionNotice: string | null;
  pendingMutation: { label: string } | null;
  composeSession: ComposeSession | null;
  composeOpen: boolean;
  setComposeOpen: StateSetter<boolean>;
  setFocusContext: StateSetter<FocusContext>;
  selectedRow: MailboxRow | null;
  openComposeShell: (kind: "new" | "reply" | "forward", messageId?: string) => Promise<void>;
  openApplyLabelDialog: () => void;
  openSnoozeDialog: () => Promise<void>;
  refreshCurrentView: (options?: { preserveReader?: boolean }) => Promise<void>;
  mailboxFilterOpen: boolean;
  mailboxFilterQuery: string;
  setMailboxFilterOpen: StateSetter<boolean>;
  setMailboxFilterQuery: StateSetter<string>;
  mailbox: MailboxPayload;
  mailboxRows: FlattenedEntry[];
  mailListMode: "threads" | "messages";
  selectedMailboxThreadId: string | null;
  selectedMessageIds: Set<string>;
  pendingMessageIds: Set<string>;
  setSelectedMailboxThreadId: StateSetter<string | null>;
  openThread: () => void;
  layoutMode: LayoutMode;
  thread: ThreadResponse | null;
  effectiveReaderMode: ReaderMode;
  setReaderMode: StateSetter<ReaderMode>;
  signatureExpanded: boolean;
  archiveSelected: () => Promise<void>;
  closeReader: () => void;
  utilityRail: UtilityRailPayload;
  searchInputRef: RefObject<HTMLInputElement | null>;
  searchScope: SearchScope;
  setSearchScope: StateSetter<SearchScope>;
  searchMode: SearchMode;
  setSearchMode: StateSetter<SearchMode>;
  searchSort: SearchSort;
  setSearchSort: StateSetter<SearchSort>;
  searchExplain: boolean;
  setSearchExplain: StateSetter<boolean>;
  searchState: SearchResponse;
  searchRows: FlattenedEntry[];
  loadMoreSearch: () => Promise<void>;
  selectedSearchThreadId: string | null;
  setSelectedSearchThreadId: StateSetter<string | null>;
  rulesState: RulesResponse;
  selectedRuleId: string | null;
  rulePanelMode: "details" | "history" | "dryRun";
  ruleDetail: Record<string, unknown> | null;
  ruleHistoryState: Array<Record<string, unknown>>;
  ruleDryRunState: Array<Record<string, unknown>>;
  ruleStatus: string | null;
  setSelectedRuleId: StateSetter<string | null>;
  openRuleForm: (mode: "new" | "edit") => Promise<void>;
  toggleSelectedRuleEnabled: () => Promise<void>;
  openRuleHistory: () => Promise<void>;
  openRuleDryRun: () => Promise<void>;
  deleteSelectedRule: () => Promise<void>;
  accountsState: AccountsResponse;
  selectedAccountId: string | null;
  accountStatus: string | null;
  accountResult: AccountOperationResponse["result"] | null;
  setSelectedAccountId: StateSetter<string | null>;
  openAccountForm: () => void;
  testCurrentAccount: () => Promise<void>;
  makeSelectedAccountDefault: () => Promise<void>;
  readyBridge: Extract<BridgeState, { kind: "ready" }>;
  eventStreamStatus: ConnectionStatus;
  contextMenu: ReturnType<typeof useContextMenu>;
  diagnosticsState: DiagnosticsResponse | null;
  generateBugReport: () => Promise<void>;
  focusContext: FocusContext;
  commandPaletteOpen: boolean;
  commandInputRef: RefObject<HTMLInputElement | null>;
  commandQuery: string;
  setCommandQuery: StateSetter<string>;
  filteredCommands: ReadonlyArray<{
    action: string;
    category: string;
    label: string;
    shortcut: string;
  }>;
  selectedCommandIndex: number;
  setSelectedCommandIndex: StateSetter<number>;
  runSelectedCommand: () => void;
  dispatchAction: (action: DesktopAction | string) => void;
  setCommandPaletteOpen: StateSetter<boolean>;
  helpOpen: boolean;
  helpSections: ReadonlyArray<{
    id: string;
    title: string;
    entries: ReadonlyArray<{ display: string; action: string; label: string }>;
  }>;
  setHelpOpen: StateSetter<boolean>;
  showInboxZero: boolean;
  setShowInboxZero: StateSetter<boolean>;
  composeDraft: ComposeFrontmatter | null;
  composeBusy: string | null;
  composeError: string | null;
  setComposeDraft: StateSetter<ComposeFrontmatter | null>;
  closeComposeShell: () => void;
  launchComposeEditor: () => Promise<void>;
  refreshComposeSession: () => Promise<void>;
  submitComposeAction: (
    path: "/compose/session/send" | "/compose/session/save",
    successMessage: string,
  ) => Promise<void>;
  persistComposeDraft: () => Promise<ComposeSession | null>;
  discardComposeSession: () => Promise<void>;
  setComposeBody: (body: string) => void;
  labelDialogOpen: boolean;
  labelOptions: string[];
  selectedLabels: string[];
  customLabel: string;
  setLabelDialogOpen: StateSetter<boolean>;
  setSelectedLabels: StateSetter<string[]>;
  setCustomLabel: StateSetter<string>;
  applyLabels: () => Promise<void>;
  moveDialogOpen: boolean;
  moveTargetLabel: string;
  setMoveDialogOpen: StateSetter<boolean>;
  setMoveTargetLabel: StateSetter<string>;
  moveSelectedMessage: () => Promise<void>;
  snoozeDialogOpen: boolean;
  snoozePresets: SnoozePreset[];
  selectedSnooze: string;
  setSnoozeDialogOpen: StateSetter<boolean>;
  setSelectedSnooze: StateSetter<string>;
  snoozeSelectedMessage: () => Promise<void>;
  unsubscribeDialogOpen: boolean;
  setUnsubscribeDialogOpen: StateSetter<boolean>;
  confirmUnsubscribe: () => Promise<void>;
  goToLabelOpen: boolean;
  jumpLabelOptions: SidebarItem[];
  jumpTargetLabel: string;
  setGoToLabelOpen: StateSetter<boolean>;
  setJumpTargetLabel: StateSetter<string>;
  applyJumpTarget: () => Promise<void>;
  savedSearchDialogOpen: boolean;
  savedSearchName: string;
  setSavedSearchDialogOpen: StateSetter<boolean>;
  setSavedSearchName: StateSetter<string>;
  submitSavedSearch: () => Promise<void>;
  attachmentDialogOpen: boolean;
  threadAttachments: Array<{
    id: string;
    filename: string;
    size_bytes: number;
    message_id: string;
  }>;
  setAttachmentDialogOpen: StateSetter<boolean>;
  runAttachmentAction: (
    path: "/attachments/open" | "/attachments/download",
    attachmentId: string,
    messageId: string,
  ) => Promise<void>;
  linksDialogOpen: boolean;
  threadLinks: string[];
  setLinksDialogOpen: StateSetter<boolean>;
  openExternalUrl: (url: string) => Promise<void>;
  reportOpen: boolean;
  reportTitle: string;
  reportContent: string;
  setReportOpen: StateSetter<boolean>;
  ruleFormOpen: boolean;
  ruleFormBusy: string | null;
  ruleFormState: RuleFormPayload;
  setRuleFormOpen: StateSetter<boolean>;
  setRuleFormState: StateSetter<RuleFormPayload>;
  saveRuleForm: () => Promise<void>;
  accountFormOpen: boolean;
  accountFormBusy: string | null;
  accountDraftJson: string;
  setAccountFormOpen: StateSetter<boolean>;
  setAccountDraftJson: StateSetter<string>;
  saveAccountDraft: () => Promise<void>;
}) {
  return (
    <div className="flex h-dvh bg-canvas text-foreground">
      <div className="flex min-w-0 flex-1 flex-col">
        <WorkbenchHeader
          screen={props.screen}
          screens={SCREEN_ORDER}
          onSwitch={props.switchScreen}
          statusMessage={props.shell.statusMessage}
          pendingBindingTokens={props.pendingBinding?.tokens ?? null}
          actionNotice={props.actionNotice}
          pendingMutationLabel={props.pendingMutation?.label ?? null}
          canResumeDraft={Boolean(props.composeSession && !props.composeOpen)}
          onResumeDraft={() => {
            props.setComposeOpen(true);
            props.setFocusContext("compose");
          }}
          onSync={() => void props.refreshCurrentView({ preserveReader: true })}
          onCompose={() => void props.openComposeShell("new")}
          onReply={() =>
            props.selectedRow && void props.openComposeShell("reply", props.selectedRow.id)
          }
          onForward={() =>
            props.selectedRow && void props.openComposeShell("forward", props.selectedRow.id)
          }
          onLabel={() => props.selectedRow && props.openApplyLabelDialog()}
          onSnooze={() => props.selectedRow && void props.openSnoozeDialog()}
          selectedRowAvailable={Boolean(props.selectedRow)}
          accountLabel={props.shell.accountLabel}
          syncLabel={props.shell.syncLabel}
        />

        <div className="flex min-h-0 min-w-0 flex-1">
          {props.screen === "mailbox" ? (
            <NavigationSidebar
              unreadCount={props.mailbox.counts.unread}
              sidebar={props.sidebar}
              accountLabel={props.shell.accountLabel}
              accounts={props.accountsState.accounts.map((a) => ({ key: a.key ?? a.account_id, name: a.name, is_default: a.is_default }))}
              onSwitchAccount={async (key) => {
                await fetchJson(props.readyBridge.baseUrl, props.readyBridge.authToken, "/accounts/default", {
                  method: "POST",
                  body: JSON.stringify({ key }),
                });
                await props.refreshCurrentView({ preserveReader: false });
              }}
              onApplySidebarLens={(item) => void props.applySidebarLens(item)}
            />
          ) : null}

          <main className="flex min-w-0 flex-1 flex-col">
            <WorkbenchContent
              screen={props.screen}
              mailbox={props.mailbox}
              mailboxRows={props.mailboxRows}
              mailListMode={props.mailListMode}
              selectedMailboxThreadId={props.selectedMailboxThreadId}
              selectedMessageIds={props.selectedMessageIds}
              pendingMessageIds={props.pendingMessageIds}
              onSelectMailboxThread={(threadId) => {
                props.setSelectedMailboxThreadId(threadId);
                props.setFocusContext("mailList");
              }}
              onOpenThread={props.openThread}
              layoutMode={props.layoutMode}
              thread={props.thread}
              readerMode={props.effectiveReaderMode}
              setReaderMode={props.setReaderMode}
              signatureExpanded={props.signatureExpanded}
              onArchive={() => void props.archiveSelected()}
              onCloseReader={props.closeReader}
              utilityRail={props.utilityRail}
              filterQuery={props.mailboxFilterQuery}
              filterOpen={props.mailboxFilterOpen}
              onFilterChange={(q) => props.setMailboxFilterQuery(q)}
              onFilterClose={() => {
                props.setMailboxFilterOpen(false);
                props.setMailboxFilterQuery("");
              }}
              onRowContextMenu={(e, threadId) => {
                props.setSelectedMailboxThreadId(threadId);
                props.contextMenu.show(e, [
                  { label: "Archive", shortcut: "E", onClick: () => props.dispatchAction("archive") },
                  { label: "Star", shortcut: "S", onClick: () => props.dispatchAction("star") },
                  { label: "Mark read", shortcut: "I", onClick: () => props.dispatchAction("mark_read"), separator: true },
                  { label: "Apply label", shortcut: "L", onClick: () => props.dispatchAction("apply_label") },
                  { label: "Move to", shortcut: "V", onClick: () => props.dispatchAction("move_label") },
                  { label: "Snooze", shortcut: "Z", onClick: () => props.dispatchAction("snooze"), separator: true },
                  { label: "Reply", shortcut: "R", onClick: () => props.dispatchAction("reply") },
                  { label: "Reply all", shortcut: "A", onClick: () => props.dispatchAction("reply_all") },
                  { label: "Forward", shortcut: "F", onClick: () => props.dispatchAction("forward"), separator: true },
                  { label: "Open in browser", shortcut: "O", onClick: () => props.dispatchAction("open_in_browser") },
                  { label: "Export", shortcut: "E", onClick: () => props.dispatchAction("export_thread"), separator: true },
                  { label: "Spam", shortcut: "!", danger: true, onClick: () => props.dispatchAction("spam") },
                  { label: "Trash", shortcut: "#", danger: true, onClick: () => props.dispatchAction("trash") },
                ]);
              }}
              searchInputRef={props.searchInputRef}
              searchQuery={props.searchQuery}
              onSearchQueryChange={props.setSearchQuery}
              searchScope={props.searchScope}
              onSearchScopeChange={props.setSearchScope}
              searchMode={props.searchMode}
              onSearchModeChange={props.setSearchMode}
              searchSort={props.searchSort}
              onSearchSortChange={props.setSearchSort}
              searchExplain={props.searchExplain}
              onSearchExplainChange={props.setSearchExplain}
              searchState={props.searchState}
              searchRows={props.searchRows}
              selectedSearchThreadId={props.selectedSearchThreadId}
              onSelectSearchThread={(threadId) => {
                props.setSelectedSearchThreadId(threadId);
                props.setFocusContext("search");
              }}
              onLoadMoreSearch={props.loadMoreSearch}
              rulesState={props.rulesState}
              selectedRuleId={props.selectedRuleId}
              rulePanelMode={props.rulePanelMode}
              ruleDetail={props.ruleDetail}
              ruleHistoryState={props.ruleHistoryState}
              ruleDryRunState={props.ruleDryRunState}
              ruleStatus={props.ruleStatus}
              onSelectRule={props.setSelectedRuleId}
              onNewRule={() => void props.openRuleForm("new")}
              onEditRule={() => void props.openRuleForm("edit")}
              onToggleRule={() => void props.toggleSelectedRuleEnabled()}
              onRuleHistory={() => void props.openRuleHistory()}
              onRuleDryRun={() => void props.openRuleDryRun()}
              onDeleteRule={() => void props.deleteSelectedRule()}
              accountsState={props.accountsState}
              selectedAccountId={props.selectedAccountId}
              accountStatus={props.accountStatus}
              accountResult={props.accountResult}
              onSelectAccount={props.setSelectedAccountId}
              onNewAccount={props.openAccountForm}
              onTestAccount={() => void props.testCurrentAccount()}
              onSetDefaultAccount={() => void props.makeSelectedAccountDefault()}
              bridge={props.readyBridge}
              diagnosticsState={props.diagnosticsState}
              onGenerateBugReport={() => void props.generateBugReport()}
            />

            <WorkbenchStatusBar
              hints={buildStatusHints(
                props.screen,
                props.focusContext,
                props.selectedMessageIds.size,
                props.layoutMode,
              )}
              screen={props.screen}
              layoutMode={props.layoutMode}
              focusContext={props.focusContext}
              commandHint={props.shell.commandHint}
              totalThreads={props.mailbox.counts.total}
              eventStreamStatus={props.eventStreamStatus}
            />
          </main>
        </div>
      </div>

      <CommandPaletteOverlay
        open={props.commandPaletteOpen}
        inputRef={props.commandInputRef}
        query={props.commandQuery}
        onQueryChange={props.setCommandQuery}
        commands={props.filteredCommands}
        selectedIndex={props.selectedCommandIndex}
        onHighlight={props.setSelectedCommandIndex}
        onSelect={(action) => {
          props.dispatchAction(action);
          props.setCommandPaletteOpen(false);
          props.setCommandQuery("");
        }}
      />

      <HelpOverlay
        open={props.helpOpen}
        sections={props.helpSections}
        onClose={() => props.setHelpOpen(false)}
      />

      <InboxZeroOverlay
        open={props.showInboxZero}
        onDismiss={() => props.setShowInboxZero(false)}
      />

      <DesktopDialogs
        screen={props.screen}
        selectedRowSender={props.selectedRow?.sender ?? null}
        composeOpen={props.composeOpen}
        composeSession={props.composeSession}
        composeDraft={props.composeDraft}
        composeBusy={props.composeBusy}
        composeError={props.composeError}
        utilityRail={props.utilityRail}
        onComposeDraftChange={props.setComposeDraft}
        onCloseCompose={props.closeComposeShell}
        onOpenComposeEditor={() => void props.launchComposeEditor()}
        onRefreshCompose={() => void props.refreshComposeSession()}
        onSendCompose={() => void props.submitComposeAction("/compose/session/send", "Sent")}
        onSaveCompose={() => void props.submitComposeAction("/compose/session/save", "Draft saved")}
        onDiscardCompose={() => void props.discardComposeSession()}
        onPersistComposeDraft={async () => { await props.persistComposeDraft(); }}
        onComposeBodyChange={props.setComposeBody}
        fetchContactSuggestions={async (query) => {
          try {
            const data = await fetchJson<SearchResponse>(
              props.readyBridge.baseUrl,
              props.readyBridge.authToken,
              `/search?q=from:${encodeURIComponent(query)}&scope=messages&mode=lexical&sort=recent&limit=10`,
            );
            const seen = new Set<string>();
            const results: Array<{ label: string; value: string }> = [];
            for (const group of data.groups) {
              for (const row of group.rows) {
                const email = row.sender_detail ?? row.sender;
                if (!seen.has(email)) {
                  seen.add(email);
                  results.push({ label: row.sender, value: email });
                }
              }
            }
            return results;
          } catch {
            return [];
          }
        }}
        knownSenders={buildKnownSenders(props.mailbox.groups)}
        labelDialogOpen={props.labelDialogOpen}
        labelOptions={props.labelOptions}
        selectedLabels={props.selectedLabels}
        customLabel={props.customLabel}
        onCloseLabelDialog={() => {
          props.setLabelDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        onToggleLabel={(label) =>
          props.setSelectedLabels((current) =>
            current.includes(label)
              ? current.filter((value) => value !== label)
              : [...current, label],
          )
        }
        onCustomLabelChange={props.setCustomLabel}
        onSubmitLabels={() => void props.applyLabels()}
        moveDialogOpen={props.moveDialogOpen}
        moveTargetLabel={props.moveTargetLabel}
        onCloseMoveDialog={() => {
          props.setMoveDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        onMoveTargetChange={props.setMoveTargetLabel}
        onSubmitMove={() => void props.moveSelectedMessage()}
        snoozeDialogOpen={props.snoozeDialogOpen}
        snoozePresets={props.snoozePresets}
        selectedSnooze={props.selectedSnooze}
        onCloseSnoozeDialog={() => {
          props.setSnoozeDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        onSelectedSnoozeChange={props.setSelectedSnooze}
        onSubmitSnooze={() => void props.snoozeSelectedMessage()}
        unsubscribeDialogOpen={props.unsubscribeDialogOpen}
        onCloseUnsubscribeDialog={() => {
          props.setUnsubscribeDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        onSubmitUnsubscribe={() => void props.confirmUnsubscribe()}
        goToLabelOpen={props.goToLabelOpen}
        jumpLabelOptions={props.jumpLabelOptions}
        jumpTargetLabel={props.jumpTargetLabel}
        onCloseGoToLabelDialog={() => {
          props.setGoToLabelOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        onJumpTargetLabelChange={props.setJumpTargetLabel}
        onSubmitJumpTarget={() => void props.applyJumpTarget()}
        savedSearchDialogOpen={props.savedSearchDialogOpen}
        savedSearchName={props.savedSearchName}
        savedSearchQuery={props.searchQuery}
        savedSearchMode={props.searchMode}
        onCloseSavedSearchDialog={() => {
          props.setSavedSearchDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        onSavedSearchNameChange={props.setSavedSearchName}
        onSubmitSavedSearch={() => void props.submitSavedSearch()}
        attachmentDialogOpen={props.attachmentDialogOpen}
        threadAttachments={props.threadAttachments}
        onCloseAttachmentDialog={() => {
          props.setAttachmentDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        onOpenAttachment={(attachmentId, messageId) =>
          void props.runAttachmentAction("/attachments/open", attachmentId, messageId)
        }
        onDownloadAttachment={(attachmentId, messageId) =>
          void props.runAttachmentAction("/attachments/download", attachmentId, messageId)
        }
        linksDialogOpen={props.linksDialogOpen}
        threadLinks={props.threadLinks}
        onCloseLinksDialog={() => {
          props.setLinksDialogOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        onOpenLink={(url) => void props.openExternalUrl(url)}
        reportOpen={props.reportOpen}
        reportTitle={props.reportTitle}
        reportContent={props.reportContent}
        onCloseReportDialog={() => {
          props.setReportOpen(false);
          props.setFocusContext(props.screen === "search" ? "search" : "mailList");
        }}
        ruleFormOpen={props.ruleFormOpen}
        ruleFormBusy={props.ruleFormBusy}
        ruleFormState={props.ruleFormState}
        onCloseRuleFormDialog={() => {
          props.setRuleFormOpen(false);
          props.setFocusContext("sidebar");
        }}
        onRuleFormChange={props.setRuleFormState}
        onSubmitRuleForm={() => void props.saveRuleForm()}
        accountFormOpen={props.accountFormOpen}
        accountFormBusy={props.accountFormBusy}
        accountDraftJson={props.accountDraftJson}
        accountResult={props.accountResult}
        onCloseAccountFormDialog={() => {
          props.setAccountFormOpen(false);
          props.setFocusContext("sidebar");
        }}
        onAccountDraftChange={props.setAccountDraftJson}
        onTestAccount={() => void props.testCurrentAccount()}
        onSaveAccount={() => void props.saveAccountDraft()}
      />
      <ContextMenuOverlay menu={props.contextMenu.menu} onClose={props.contextMenu.close} />
    </div>
  );
}

function useMailboxMutationActions(props: {
  screen: WorkbenchScreen;
  currentThreadId: string | null;
  layoutMode: LayoutMode;
  bridge: BridgeState;
  activeSidebarItem: SidebarItem | null;
  mailbox: MailboxPayload;
  searchState: SearchResponse;
  thread: ThreadResponse | null;
  effectiveSelection: string[];
  selectedRow: MailboxRow | null;
  setPendingMutation: StateSetter<{ messageIds: Set<string>; label: string } | null>;
  setMailbox: StateSetter<MailboxPayload>;
  setSearchState: StateSetter<SearchResponse>;
  setThread: StateSetter<ThreadResponse | null>;
  loadSearch: () => Promise<void>;
  loadThread: (threadId: string) => Promise<void>;
  loadMailbox: (lens?: SidebarLens, options?: { preserveReader?: boolean }) => Promise<void>;
  closeReader: () => void;
  showNotice: (message: string) => void;
}) {
  const refreshCurrentView = useEffectEvent(async (options?: { preserveReader?: boolean }) => {
    if (props.screen === "search") {
      await props.loadSearch();
      if (options?.preserveReader && props.currentThreadId && props.layoutMode !== "twoPane") {
        await props.loadThread(props.currentThreadId);
      }
      return;
    }

    await props.loadMailbox(props.activeSidebarItem?.lens, options);
    if (options?.preserveReader && props.currentThreadId && props.layoutMode !== "twoPane") {
      await props.loadThread(props.currentThreadId);
    }
  });

  const runPendingMutation = useEffectEvent(
    async (messageIds: string[], label: string, work: () => Promise<void>) => {
      props.setPendingMutation({
        messageIds: new Set(messageIds),
        label,
      });
      try {
        await work();
      } finally {
        props.setPendingMutation(null);
      }
    },
  );

  const applyOptimisticRowPatch = useEffectEvent(
    (messageIds: string[], patch: OptimisticRowPatch) => {
      const ids = new Set(messageIds);
      startTransition(() => {
        props.setMailbox((current) => patchMailboxPayload(current, ids, patch));
        props.setSearchState((current) => patchSearchResponse(current, ids, patch));
        props.setThread((current) => patchThreadResponse(current, ids, patch));
      });
    },
  );

  const mutateSelected = useEffectEvent(
    async (
      path: string,
      body: Record<string, unknown>,
      options?: {
        closeReader?: boolean;
        preserveReader?: boolean;
        optimistic?: OptimisticRowPatch;
        pendingLabel?: string;
      },
    ) => {
      if (props.bridge.kind !== "ready" || !props.selectedRow) {
        return;
      }

      const rollback = options?.optimistic
        ? {
            mailbox: props.mailbox,
            searchState: props.searchState,
            thread: props.thread,
          }
        : null;

      if (options?.optimistic) {
        applyOptimisticRowPatch(props.effectiveSelection, options.optimistic);
      }

      if (options?.closeReader && props.layoutMode !== "twoPane") {
        props.closeReader();
      }

      try {
        const { baseUrl, authToken } = props.bridge;
        await runPendingMutation(
          props.effectiveSelection,
          options?.pendingLabel ??
            formatPendingMutationLabel("Updating", props.effectiveSelection.length),
          async () => {
            await fetchJson(baseUrl, authToken, path, {
              method: "POST",
              body: JSON.stringify(body),
            });
            await refreshCurrentView({ preserveReader: options?.preserveReader });
          },
        );
      } catch (error) {
        if (rollback) {
          startTransition(() => {
            props.setMailbox(rollback.mailbox);
            props.setSearchState(rollback.searchState);
            props.setThread(rollback.thread);
          });
        }
        props.showNotice(error instanceof Error ? error.message : "Mutation failed");
      }
    },
  );

  const archiveSelected = useEffectEvent(async () => {
    if (!props.selectedRow) {
      return;
    }
    await mutateSelected(
      "/mutations/archive",
      { message_ids: props.effectiveSelection },
      {
        closeReader: true,
        pendingLabel: formatPendingMutationLabel("Archiving", props.effectiveSelection.length),
      },
    );
  });

  return {
    refreshCurrentView,
    runPendingMutation,
    applyOptimisticRowPatch,
    mutateSelected,
    archiveSelected,
  };
}

function useDesktopActionDispatcher(context: Parameters<typeof runDesktopAction>[1]) {
  return useEffectEvent((action: DesktopAction | string) => {
    runDesktopAction(action, context);
  });
}

function useWorkbenchLifecycle(props: {
  bridge: BridgeState;
  screen: WorkbenchScreen;
  searchRefreshKey: string;
  layoutMode: LayoutMode;
  currentThreadId: string | null;
  selectedRow: MailboxRow | null;
  mailbox: MailboxPayload;
  searchState: SearchResponse;
  thread: ThreadResponse | null;
  selectedRuleId: string | null;
  commandPaletteOpen: boolean;
  commandInputRef: RefObject<HTMLInputElement | null>;
  setBridge: StateSetter<BridgeState>;
  loadMailbox: (lens?: SidebarLens, options?: { preserveReader?: boolean }) => Promise<void>;
  loadSearch: () => Promise<void>;
  loadThread: (threadId: string) => Promise<void>;
  loadRules: () => Promise<void>;
  loadAccounts: () => Promise<void>;
  loadDiagnostics: () => Promise<void>;
  loadSelectedRuleDetail: (ruleId?: string | null) => Promise<void>;
  applyOptimisticRowPatch: (messageIds: string[], patch: OptimisticRowPatch) => void;
  runPendingMutation: (
    messageIds: string[],
    label: string,
    work: () => Promise<void>,
  ) => Promise<void>;
  refreshCurrentView: (options?: { preserveReader?: boolean }) => Promise<void>;
  setMailbox: StateSetter<MailboxPayload>;
  setSearchState: StateSetter<SearchResponse>;
  setThread: StateSetter<ThreadResponse | null>;
  showNotice: (message: string) => void;
}) {
  const {
    bridge,
    screen,
    searchRefreshKey,
    layoutMode,
    currentThreadId,
    selectedRow,
    mailbox,
    searchState,
    thread,
    selectedRuleId,
    commandPaletteOpen,
    commandInputRef,
    setBridge,
    loadMailbox,
    loadSearch,
    loadThread,
    loadRules,
    loadAccounts,
    loadDiagnostics,
    loadSelectedRuleDetail,
    applyOptimisticRowPatch,
    runPendingMutation,
    refreshCurrentView,
    setMailbox,
    setSearchState,
    setThread,
    showNotice,
  } = props;
  const pendingPreviewReadRef = useRef<PendingPreviewReadState | null>(null);
  const selectedRowId = selectedRow?.id ?? null;
  const selectedRowUnread = selectedRow?.unread ?? false;

  const syncBridgeState = useEffectEvent(async () => {
    setBridge(await window.mxrDesktop.getBridgeState());
  });

  const refreshMailbox = useEffectEvent(async () => {
    if (bridge.kind === "ready") {
      await loadMailbox();
    }
  });

  const refreshSearch = useEffectEvent(async () => {
    if (bridge.kind === "ready" && screen === "search") {
      await loadSearch();
    }
  });

  const refreshThread = useEffectEvent(async () => {
    if (!currentThreadId || bridge.kind !== "ready") {
      return;
    }
    if (screen !== "search" && layoutMode === "twoPane") {
      return;
    }
    await loadThread(currentThreadId);
  });

  const refreshSupportScreen = useEffectEvent(async () => {
    if (bridge.kind !== "ready") {
      return;
    }
    if (screen === "rules") {
      await loadRules();
      return;
    }
    if (screen === "accounts") {
      await loadAccounts();
      return;
    }
    if (screen === "diagnostics") {
      await loadDiagnostics();
    }
  });

  const refreshSelectedRuleDetail = useEffectEvent(async () => {
    if (screen === "rules" && selectedRuleId && bridge.kind === "ready") {
      await loadSelectedRuleDetail(selectedRuleId);
    }
  });

  useEffect(() => {
    void syncBridgeState();
  }, []);

  useEffect(() => {
    if (bridge.kind === "ready") {
      void refreshMailbox();
      void loadAccounts();
    }
  }, [bridge.kind]);


  useEffect(() => {
    if (bridge.kind === "ready" && screen === "search") {
      void refreshSearch();
    }
  }, [bridge.kind, screen, searchRefreshKey]);

  const cancelPendingPreviewRead = useEffectEvent(() => {
    const pending = pendingPreviewReadRef.current;
    if (!pending) {
      return;
    }
    window.clearTimeout(pending.timeoutId);
    pendingPreviewReadRef.current = null;
  });

  const commitPreviewRead = useEffectEvent(async (messageId: string) => {
    if (bridge.kind !== "ready" || screen !== "mailbox" || layoutMode === "twoPane") {
      return;
    }
    if (!selectedRow || selectedRow.id !== messageId || !selectedRow.unread) {
      return;
    }

    const rollback = {
      mailbox,
      searchState,
      thread,
    };

    applyOptimisticRowPatch([messageId], { unread: false });

    try {
      const { baseUrl, authToken } = bridge;
      await runPendingMutation(
        [messageId],
        formatPendingMutationLabel("Marking", 1, "read"),
        async () => {
          await fetchJson<ActionAckResponse>(baseUrl, authToken, "/mutations/read", {
            method: "POST",
            body: JSON.stringify({
              message_ids: [messageId],
              read: true,
            }),
          });
          await refreshCurrentView({ preserveReader: true });
        },
      );
    } catch (error) {
      startTransition(() => {
        setMailbox(rollback.mailbox);
        setSearchState(rollback.searchState);
        setThread(rollback.thread);
      });
      showNotice(error instanceof Error ? error.message : "Mutation failed");
    }
  });

  useEffect(() => {
    if (
      bridge.kind !== "ready" ||
      screen !== "mailbox" ||
      layoutMode === "twoPane" ||
      !selectedRowId
    ) {
      cancelPendingPreviewRead();
      return;
    }

    if (!selectedRowUnread) {
      cancelPendingPreviewRead();
      return;
    }

    if (pendingPreviewReadRef.current?.messageId === selectedRowId) {
      return;
    }

    cancelPendingPreviewRead();

    const messageId = selectedRowId;
    const timeoutId = window.setTimeout(() => {
      if (pendingPreviewReadRef.current?.messageId === messageId) {
        pendingPreviewReadRef.current = null;
      }
      void commitPreviewRead(messageId);
    }, PREVIEW_MARK_READ_DELAY_MS);

    pendingPreviewReadRef.current = { messageId, timeoutId };

    return () => {
      if (pendingPreviewReadRef.current?.messageId === messageId) {
        window.clearTimeout(timeoutId);
        pendingPreviewReadRef.current = null;
      }
    };
  }, [bridge.kind, layoutMode, screen, selectedRowId, selectedRowUnread]);

  useEffect(() => {
    void refreshThread();
  }, [bridge.kind, currentThreadId, layoutMode, screen]);

  useEffect(() => {
    void refreshSupportScreen();
  }, [bridge.kind, screen]);

  useEffect(() => {
    void refreshSelectedRuleDetail();
  }, [bridge.kind, screen, selectedRuleId]);

  useEffect(() => {
    if (!commandPaletteOpen) {
      return;
    }
    commandInputRef.current?.focus();
  }, [commandInputRef, commandPaletteOpen]);
}

function useActionNoticeTimeout(
  actionNotice: string | null,
  setActionNotice: (updater: string | null) => void,
) {
  useEffect(() => {
    if (!actionNotice) {
      return;
    }
    const timeout = window.setTimeout(() => setActionNotice(null), 2400);
    return () => window.clearTimeout(timeout);
  }, [actionNotice, setActionNotice]);
}

function useComposeWindowRefresh(
  composeOpen: boolean,
  composeSession: unknown,
  refreshComposeSession: () => Promise<void>,
) {
  const refreshOnFocus = useEffectEvent(() => {
    void refreshComposeSession();
  });

  useEffect(() => {
    if (!composeOpen || !composeSession) {
      return;
    }

    const onFocus = () => {
      refreshOnFocus();
    };

    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [composeOpen, composeSession]);
}

function usePruneSelectedMessages(
  mailboxGroups: MailboxGroup[],
  searchGroups: MailboxGroup[],
  setSelectedMessageIds: (updater: (current: Set<string>) => Set<string>) => void,
) {
  useEffect(() => {
    const available = new Set([
      ...mailboxGroups.flatMap((group) => group.rows.map((row) => row.id)),
      ...searchGroups.flatMap((group) => group.rows.map((row) => row.id)),
    ]);
    setSelectedMessageIds((current) => {
      const next = new Set([...current].filter((id) => available.has(id)));
      if (next.size === current.size) {
        return current;
      }
      return next;
    });
  }, [mailboxGroups, searchGroups, setSelectedMessageIds]);
}

function renderBridgeGate(props: {
  bridge: BridgeState;
  workbenchReady: boolean;
  externalPath: string;
  setExternalPath: (value: string) => void;
  setBridge: (state: BridgeState) => void;
  refreshBridge: () => Promise<void>;
}) {
  if (props.bridge.kind === "mismatch") {
    return (
      <BridgeMismatchView
        bridge={props.bridge}
        externalPath={props.externalPath}
        onExternalPathChange={props.setExternalPath}
        onUseBundled={() => void window.mxrDesktop.useBundledMxr().then(props.setBridge)}
        onRetry={() => void props.refreshBridge()}
        onTryExternal={() =>
          void window.mxrDesktop.setExternalBinaryPath(props.externalPath).then(props.setBridge)
        }
      />
    );
  }

  if (props.bridge.kind === "error") {
    return (
      <BridgeErrorView
        title={props.bridge.title}
        detail={props.bridge.detail}
        updateSteps={UPDATE_STEPS}
        onRetry={() => void props.refreshBridge()}
      />
    );
  }

  if (props.bridge.kind !== "ready") {
    return (
      <BridgeLoadingView
        title="Connecting to local mail runtime"
        detail="Starting the bridge, validating protocol compatibility, and loading local state."
      />
    );
  }

  if (!props.workbenchReady) {
    return (
      <BridgeLoadingView
        title="Loading local workspace"
        detail="Hydrating shell state, sidebar counts, and the active mailbox lens."
      />
    );
  }

  return null;
}

function findRowByThreadId(groups: MailboxGroup[], threadId: string | null) {
  if (!threadId) {
    return null;
  }
  return groups.flatMap((group) => group.rows).find((row) => row.thread_id === threadId) ?? null;
}

function defaultUtilityRail(
  shell: WorkbenchShellPayload,
  row: MailboxRow | null,
): UtilityRailPayload {
  return {
    title: "Recent opens",
    items: row
      ? [row.subject, row.sender, `${shell.accountLabel} account`]
      : [shell.statusMessage, shell.syncLabel, shell.commandHint],
  };
}

function findActiveSidebarItem(sidebar: SidebarPayload): SidebarItem | null {
  for (const section of sidebar.sections) {
    const match = section.items.find((item) => item.active);
    if (match) {
      return match;
    }
  }
  return null;
}

function resolveReaderMode(mode: ReaderMode, thread: ThreadResponse | null): ReaderMode {
  if (mode !== "auto") {
    return mode;
  }
  if (!thread) {
    return "reader";
  }
  if (thread.reader_mode && thread.reader_mode !== "auto") {
    return thread.reader_mode;
  }
  const htmlBody = thread.bodies.find((body) => body.text_html)?.text_html;
  const plainBody = thread.bodies.find((body) => body.text_plain)?.text_plain;
  if (htmlBody && !plainBody) {
    return "html";
  }
  return "reader";
}

function collectLabelOptions(sidebar: SidebarPayload) {
  const labels = new Set<string>();
  for (const section of sidebar.sections) {
    if (section.title !== "System" && section.title !== "Labels") {
      continue;
    }
    for (const item of section.items) {
      if (item.label === "All Mail") {
        continue;
      }
      labels.add(item.label);
    }
  }
  return [...labels];
}

function collectJumpTargets(sidebar: SidebarPayload) {
  return sidebar.sections
    .filter((section) => section.title === "System" || section.title === "Labels")
    .flatMap((section) => section.items);
}

function collectAttachments(thread: ThreadResponse | null) {
  if (!thread) {
    return [];
  }
  return thread.bodies.flatMap((body) =>
    body.attachments.map((attachment) => ({
      id: attachment.id,
      filename: attachment.filename,
      size_bytes: attachment.size_bytes,
      message_id: body.message_id,
    })),
  );
}

function collectLinks(thread: ThreadResponse | null) {
  if (!thread) {
    return [];
  }
  const matches = new Set<string>();
  const text = thread.bodies
    .flatMap((body) => [body.text_plain, body.text_html, body.raw_source])
    .filter(Boolean)
    .join("\n");
  for (const match of text.matchAll(/https?:\/\/[^\s"'<>]+/g)) {
    matches.add(match[0]);
  }
  return [...matches];
}

function patchMailboxPayload(
  payload: MailboxPayload,
  messageIds: Set<string>,
  patch: OptimisticRowPatch,
) {
  let changed = false;
  let unreadDelta = 0;

  const groups = payload.groups.map((group) => {
    let groupChanged = false;
    const rows = group.rows.map((row) => {
      if (!messageIds.has(row.id)) {
        return row;
      }

      const nextUnread = patch.unread ?? row.unread;
      const nextStarred = patch.starred ?? row.starred;
      if (nextUnread === row.unread && nextStarred === row.starred) {
        return row;
      }

      changed = true;
      groupChanged = true;
      if (nextUnread !== row.unread) {
        unreadDelta += nextUnread ? 1 : -1;
      }

      return {
        ...row,
        unread: nextUnread,
        starred: nextStarred,
      };
    });

    return groupChanged ? { ...group, rows } : group;
  });

  if (!changed) {
    return payload;
  }

  return {
    ...payload,
    counts: {
      ...payload.counts,
      unread: Math.max(0, payload.counts.unread + unreadDelta),
    },
    groups,
  };
}

function patchSearchResponse(
  response: SearchResponse,
  messageIds: Set<string>,
  patch: OptimisticRowPatch,
) {
  let changed = false;

  const groups = response.groups.map((group) => {
    let groupChanged = false;
    const rows = group.rows.map((row) => {
      if (!messageIds.has(row.id)) {
        return row;
      }

      const nextUnread = patch.unread ?? row.unread;
      const nextStarred = patch.starred ?? row.starred;
      if (nextUnread === row.unread && nextStarred === row.starred) {
        return row;
      }

      changed = true;
      groupChanged = true;
      return {
        ...row,
        unread: nextUnread,
        starred: nextStarred,
      };
    });

    return groupChanged ? { ...group, rows } : group;
  });

  return changed ? { ...response, groups } : response;
}

function patchThreadResponse(
  thread: ThreadResponse | null,
  messageIds: Set<string>,
  patch: OptimisticRowPatch,
) {
  if (!thread) {
    return thread;
  }

  let changed = false;
  const messages = thread.messages.map((message) => {
    if (!messageIds.has(message.id)) {
      return message;
    }

    const nextUnread = patch.unread ?? message.unread;
    const nextStarred = patch.starred ?? message.starred;
    if (nextUnread === message.unread && nextStarred === message.starred) {
      return message;
    }

    changed = true;
    return {
      ...message,
      unread: nextUnread,
      starred: nextStarred,
    };
  });

  return changed ? { ...thread, messages } : thread;
}

function formatPendingMutationLabel(verb: string, count: number, detail?: string) {
  const unit = count === 1 ? "message" : "messages";
  return detail ? `${verb} ${count} ${unit} ${detail}` : `${verb} ${count} ${unit}`;
}

function buildStatusHints(
  screen: WorkbenchScreen,
  focusContext: FocusContext,
  selectedCount: number,
  layoutMode: LayoutMode,
) {
  if (screen === "search") {
    return [
      { key: "j", label: "Down" },
      { key: "k", label: "Up" },
      { key: "o", label: "Open" },
      { key: "/", label: "Search" },
      { key: "Tab", label: layoutMode === "twoPane" ? "Pane" : "Next" },
      { key: "?", label: "Help" },
    ];
  }

  if (screen === "rules") {
    return [
      { key: "j", label: "Down" },
      { key: "k", label: "Up" },
      { key: "n", label: "New" },
      { key: "E", label: "Edit" },
      { key: "D", label: "Dry Run" },
      { key: "H", label: "History" },
    ];
  }

  if (screen === "accounts") {
    return [
      { key: "n", label: "New" },
      { key: "t", label: "Test" },
      { key: "d", label: "Default" },
      { key: "Enter", label: "Edit" },
      { key: "r", label: "Refresh" },
      { key: "?", label: "Help" },
    ];
  }

  if (screen === "diagnostics") {
    return [
      { key: "Tab", label: "Pane" },
      { key: "d", label: "Details" },
      { key: "r", label: "Refresh" },
      { key: "b", label: "Bug" },
      { key: "gL", label: "Logs" },
      { key: "?", label: "Help" },
    ];
  }

  if (selectedCount > 0) {
    return [
      { key: "Esc", label: "Clear" },
      { key: "e", label: "Archive" },
      { key: "l", label: "Label" },
      { key: "v", label: "Move" },
      { key: "I", label: "Read" },
      { key: "U", label: "Unread" },
    ];
  }

  if (focusContext === "reader" && layoutMode !== "twoPane") {
    return [
      { key: "j", label: "Next" },
      { key: "k", label: "Prev" },
      { key: "r", label: "Reply" },
      { key: "a", label: "Reply All" },
      { key: "f", label: "Forward" },
      { key: "R", label: "Reader" },
    ];
  }

  if (focusContext === "sidebar") {
    return [
      { key: "j", label: "Down" },
      { key: "k", label: "Up" },
      { key: "Enter", label: "Open" },
      { key: "Tab", label: "Pane" },
      { key: "?", label: "Help" },
    ];
  }

  return [
    { key: "j", label: "Down" },
    { key: "k", label: "Up" },
    { key: "o", label: "Open" },
    { key: "r", label: "Reply" },
    { key: "l", label: "Label" },
    { key: "/", label: "Search" },
    { key: "x", label: "Select" },
  ];
}

function formatDiagnosticsDetails(report: DiagnosticsResponse["report"]) {
  const lines = [
    `Health: ${report.health_class}`,
    `Protocol: ${report.daemon_protocol_version}`,
    report.daemon_version ? `Daemon version: ${report.daemon_version}` : null,
    report.daemon_build_id ? `Build id: ${report.daemon_build_id}` : null,
    report.database_path ? `Database: ${report.database_path}` : null,
    report.index_path ? `Index: ${report.index_path}` : null,
    report.log_path ? `Log file: ${report.log_path}` : null,
    typeof report.log_size_bytes === "number" ? `Log size: ${report.log_size_bytes} bytes` : null,
    `Lexical index: ${report.lexical_index_freshness}`,
    `Semantic index: ${report.semantic_index_freshness}`,
    "",
    "Recommended next steps:",
    ...(report.recommended_next_steps.length > 0 ? report.recommended_next_steps : ["None"]),
    "",
    "Recent errors:",
    ...(report.recent_error_logs.length > 0 ? report.recent_error_logs : ["None"]),
  ];

  return lines.filter((line): line is string => line !== null).join("\n");
}
