import type {
  AccountOperationResponse,
  AccountsResponse,
  BridgeReadyState,
  DiagnosticsResponse,
  LayoutMode,
  MailboxPayload,
  ReaderMode,
  RulesResponse,
  SearchMode,
  SearchResponse,
  SearchScope,
  SearchSort,
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
  selectedMailboxThreadId: string | null;
  selectedMessageIds: Set<string>;
  pendingMessageIds: Set<string>;
  onSelectMailboxThread: (threadId: string | null) => void;
  onOpenThread: () => void;
  layoutMode: LayoutMode;
  thread: ThreadResponse | null;
  readerMode: ReaderMode;
  setReaderMode: (mode: ReaderMode | ((current: ReaderMode) => ReaderMode)) => void;
  signatureExpanded: boolean;
  onArchive: () => void;
  onCloseReader: () => void;
  utilityRail: UtilityRailPayload;
  searchInputRef: React.RefObject<HTMLInputElement | null>;
  searchQuery: string;
  onSearchQueryChange: (value: string) => void;
  searchScope: SearchScope;
  onSearchScopeChange: (value: SearchScope) => void;
  searchMode: SearchMode;
  onSearchModeChange: (value: SearchMode | ((current: SearchMode) => SearchMode)) => void;
  searchSort: SearchSort;
  onSearchSortChange: (value: SearchSort) => void;
  searchExplain: boolean;
  onSearchExplainChange: (value: boolean) => void;
  searchState: SearchResponse;
  searchRows: FlattenedEntry[];
  selectedSearchThreadId: string | null;
  onSelectSearchThread: (threadId: string | null) => void;
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
  diagnosticsState: DiagnosticsResponse | null;
  onGenerateBugReport: () => void;
}) {
  return (
    <section className="min-h-0 flex-1 overflow-hidden">
      {props.screen === "mailbox" ? (
        <MailboxWorkspace
          mailbox={props.mailbox}
          rows={props.mailboxRows}
          mailListMode={props.mailListMode}
          selectedThreadId={props.selectedMailboxThreadId}
          selectedMessageIds={props.selectedMessageIds}
          pendingMessageIds={props.pendingMessageIds}
          onSelect={props.onSelectMailboxThread}
          onOpen={props.onOpenThread}
          layoutMode={props.layoutMode}
          thread={props.thread}
          readerMode={props.readerMode}
          setReaderMode={props.setReaderMode}
          signatureExpanded={props.signatureExpanded}
          onArchive={props.onArchive}
          onCloseReader={props.onCloseReader}
          utilityRail={props.utilityRail}
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
          thread={props.thread}
          readerMode={props.readerMode}
          setReaderMode={props.setReaderMode}
          signatureExpanded={props.signatureExpanded}
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
          onGenerateBugReport={props.onGenerateBugReport}
        />
      ) : null}
    </section>
  );
}
