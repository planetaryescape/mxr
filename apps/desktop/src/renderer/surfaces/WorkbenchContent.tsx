import type {
  AccountOperationResponse,
  AccountsResponse,
  BridgeReadyState,
  DiagnosticsWorkspaceSection,
  DiagnosticsWorkspaceState,
  LayoutMode,
  MailboxPayload,
  ReaderMode,
  RulesResponse,
  SavedDraftSummary,
  SearchMode,
  SearchResponse,
  SearchScope,
  SearchSort,
  SidebarItem,
  SnoozedMessageSummary,
  SubscriptionSummary,
  ThreadResponse,
  UtilityRailPayload,
  WorkbenchScreen,
} from "../../shared/types";
import { AccountsWorkspace } from "./AccountsWorkspace";
import { DiagnosticsWorkspace } from "./DiagnosticsWorkspace";
import { MailboxWorkspace } from "./MailboxWorkspace";
import { RulesWorkspace } from "./RulesWorkspace";
import { SearchWorkspace } from "./SearchWorkspace";
import type { FlattenedEntry } from "./types";

export function WorkbenchContent(props: {
  screen: WorkbenchScreen;
  mailbox: MailboxPayload;
  mailboxRows: FlattenedEntry[];
  mailListMode: "threads" | "messages";
  mailboxLoadingLabel: string | null;
  onMailListModeChange: (mode: "threads" | "messages") => void;
  selectedMailboxThreadId: string | null;
  selectedMessageIds: Set<string>;
  pendingMessageIds: Set<string>;
  onSelectMailboxThread: (threadId: string | null) => void;
  onOpenThread: () => void;
  layoutMode: LayoutMode;
  thread: ThreadResponse | null;
  readerMode: ReaderMode;
  setReaderMode: (
    mode: ReaderMode | ((current: ReaderMode) => ReaderMode),
  ) => void;
  remoteContentEnabled: boolean;
  setRemoteContentEnabled: (value: boolean) => void;
  signatureExpanded: boolean;
  onArchive: () => void;
  onCloseReader: () => void;
  utilityRail: UtilityRailPayload;
  filterQuery: string;
  filterOpen: boolean;
  onFilterChange: (query: string) => void;
  onFilterClose: () => void;
  onRowContextMenu?: (e: React.MouseEvent, threadId: string) => void;
  searchInputRef: React.RefObject<HTMLInputElement | null>;
  searchQuery: string;
  onSearchQueryChange: (value: string) => void;
  searchScope: SearchScope;
  onSearchScopeChange: (value: SearchScope) => void;
  searchMode: SearchMode;
  onSearchModeChange: (
    value: SearchMode | ((current: SearchMode) => SearchMode),
  ) => void;
  searchSort: SearchSort;
  onSearchSortChange: (value: SearchSort) => void;
  searchExplain: boolean;
  onSearchExplainChange: (value: boolean) => void;
  searchState: SearchResponse;
  searchRows: FlattenedEntry[];
  selectedSearchThreadId: string | null;
  onSelectSearchThread: (threadId: string | null) => void;
  onLoadMoreSearch?: () => void;
  onOpenSearchAttachment?: (attachmentId: string, messageId: string) => void;
  onDownloadSearchAttachment?: (
    attachmentId: string,
    messageId: string,
  ) => void;
  rulesState: RulesResponse;
  selectedRuleId: string | null;
  rulePanelMode: "details" | "history" | "dryRun";
  ruleDetail: Record<string, unknown> | null;
  ruleHistoryState: Array<Record<string, unknown>>;
  ruleDryRunState: Array<Record<string, unknown>>;
  ruleStatus: string | null;
  onSelectRule: (ruleId: string | null) => void;
  onNewRule: () => void;
  onEditRule: () => void;
  onToggleRule: () => void;
  onRuleHistory: () => void;
  onRuleDryRun: () => void;
  onDeleteRule: () => void;
  accountsState: AccountsResponse;
  selectedAccountId: string | null;
  accountStatus: string | null;
  accountResult: AccountOperationResponse["result"] | null;
  onSelectAccount: (accountId: string | null) => void;
  onNewAccount: () => void;
  onTestAccount: () => void;
  onSetDefaultAccount: () => void;
  bridge: BridgeReadyState;
  diagnosticsState: DiagnosticsWorkspaceState | null;
  diagnosticsSection: DiagnosticsWorkspaceSection;
  onDiagnosticsSectionChange: (section: DiagnosticsWorkspaceSection) => void;
  onGenerateBugReport: () => void;
  labels: SidebarItem[];
  savedSearches: SidebarItem[];
  onResumeSavedDraft: (draft: SavedDraftSummary) => void;
  onOpenSubscription: (subscription: SubscriptionSummary) => void;
  onOpenSnoozed: (message: SnoozedMessageSummary) => void;
  onSemanticReindex: () => void;
  onCreateLabel: (name: string) => void;
  onRenameLabel: (oldName: string, newName: string) => void;
  onDeleteLabel: (name: string) => void;
  onDeleteSavedSearch: (name: string) => void;
}) {
  return (
    <section className="min-h-0 flex-1 overflow-hidden">
      {props.screen === "mailbox" ? (
        <MailboxWorkspace
          mailbox={props.mailbox}
          rows={props.mailboxRows}
          mailListMode={props.mailListMode}
          loadingLabel={props.mailboxLoadingLabel}
          onMailListModeChange={props.onMailListModeChange}
          selectedThreadId={props.selectedMailboxThreadId}
          selectedMessageIds={props.selectedMessageIds}
          pendingMessageIds={props.pendingMessageIds}
          onSelect={props.onSelectMailboxThread}
          onOpen={props.onOpenThread}
          layoutMode={props.layoutMode}
          thread={props.thread}
          readerMode={props.readerMode}
          setReaderMode={props.setReaderMode}
          remoteContentEnabled={props.remoteContentEnabled}
          setRemoteContentEnabled={props.setRemoteContentEnabled}
          signatureExpanded={props.signatureExpanded}
          onArchive={props.onArchive}
          onCloseReader={props.onCloseReader}
          utilityRail={props.utilityRail}
          filterQuery={props.filterQuery}
          filterOpen={props.filterOpen}
          onFilterChange={props.onFilterChange}
          onFilterClose={props.onFilterClose}
          onRowContextMenu={props.onRowContextMenu}
        />
      ) : null}

      {props.screen === "search" ? (
        <SearchWorkspace
          inputRef={props.searchInputRef}
          query={props.searchQuery}
          onQueryChange={props.onSearchQueryChange}
          scope={props.searchScope}
          onScopeChange={props.onSearchScopeChange}
          mode={props.searchMode}
          onModeChange={props.onSearchModeChange}
          sort={props.searchSort}
          onSortChange={props.onSearchSortChange}
          explain={props.searchExplain}
          onExplainChange={props.onSearchExplainChange}
          state={props.searchState}
          rows={props.searchRows}
          selectedMessageIds={props.selectedMessageIds}
          pendingMessageIds={props.pendingMessageIds}
          selectedThreadId={props.selectedSearchThreadId}
          onSelect={props.onSelectSearchThread}
          onOpen={props.onOpenThread}
          layoutMode={props.layoutMode}
          thread={props.thread}
          readerMode={props.readerMode}
          setReaderMode={props.setReaderMode}
          remoteContentEnabled={props.remoteContentEnabled}
          setRemoteContentEnabled={props.setRemoteContentEnabled}
          signatureExpanded={props.signatureExpanded}
          onArchive={props.onArchive}
          onCloseReader={props.onCloseReader}
          onLoadMore={props.onLoadMoreSearch}
          onOpenAttachment={props.onOpenSearchAttachment}
          onDownloadAttachment={props.onDownloadSearchAttachment}
        />
      ) : null}

      {props.screen === "rules" ? (
        <RulesWorkspace
          rules={props.rulesState.rules}
          selectedRuleId={props.selectedRuleId}
          panelMode={props.rulePanelMode}
          detail={props.ruleDetail}
          history={props.ruleHistoryState}
          dryRun={props.ruleDryRunState}
          status={props.ruleStatus}
          onSelect={props.onSelectRule}
          onNew={props.onNewRule}
          onEdit={props.onEditRule}
          onToggle={props.onToggleRule}
          onHistory={props.onRuleHistory}
          onDryRun={props.onRuleDryRun}
          onDelete={props.onDeleteRule}
        />
      ) : null}

      {props.screen === "accounts" ? (
        <AccountsWorkspace
          accounts={props.accountsState.accounts}
          selectedAccountId={props.selectedAccountId}
          status={props.accountStatus}
          result={props.accountResult}
          onSelect={props.onSelectAccount}
          onNew={props.onNewAccount}
          onTest={props.onTestAccount}
          onSetDefault={props.onSetDefaultAccount}
        />
      ) : null}

      {props.screen === "diagnostics" ? (
        <DiagnosticsWorkspace
          bridge={props.bridge}
          diagnostics={props.diagnosticsState}
          activeSection={props.diagnosticsSection}
          onSectionChange={props.onDiagnosticsSectionChange}
          onGenerateBugReport={props.onGenerateBugReport}
          labels={props.labels}
          savedSearches={props.savedSearches}
          onResumeDraft={props.onResumeSavedDraft}
          onOpenSubscription={props.onOpenSubscription}
          onOpenSnoozed={props.onOpenSnoozed}
          onSemanticReindex={props.onSemanticReindex}
          onCreateLabel={props.onCreateLabel}
          onRenameLabel={props.onRenameLabel}
          onDeleteLabel={props.onDeleteLabel}
          onDeleteSavedSearch={props.onDeleteSavedSearch}
        />
      ) : null}
    </section>
  );
}
