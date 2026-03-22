import { useReducer } from "react";
import type { SetStateAction } from "react";
import type {
  AccountsResponse,
  BridgeState,
  DiagnosticsResponse,
  FocusContext,
  LayoutMode,
  MailboxPayload,
  ReaderMode,
  RulesResponse,
  SearchResponse,
  SidebarPayload,
  ThreadResponse,
  WorkbenchScreen,
  WorkbenchShellPayload,
} from "../../shared/types";
import { objectStateReducer, updateField } from "./objectState";

type WorkbenchCoreState = {
  bridge: BridgeState;
  externalPath: string;
  screen: WorkbenchScreen;
  layoutMode: LayoutMode;
  focusContext: FocusContext;
  readerMode: ReaderMode;
  shell: WorkbenchShellPayload;
  sidebar: SidebarPayload;
  mailbox: MailboxPayload;
  searchState: SearchResponse;
  selectedMailboxThreadId: string | null;
  selectedSearchThreadId: string | null;
  thread: ThreadResponse | null;
  rulesState: RulesResponse;
  accountsState: AccountsResponse;
  diagnosticsState: DiagnosticsResponse | null;
};

export function useWorkbenchCoreState(props: {
  emptyShell: WorkbenchShellPayload;
  emptySidebar: SidebarPayload;
  emptyMailbox: MailboxPayload;
  emptySearch: SearchResponse;
}) {
  const [state, dispatch] = useReducer(objectStateReducer<WorkbenchCoreState>, {
    bridge: { kind: "idle" },
    externalPath: "",
    screen: "mailbox",
    layoutMode: "twoPane",
    focusContext: "mailList",
    readerMode: "auto",
    shell: props.emptyShell,
    sidebar: props.emptySidebar,
    mailbox: props.emptyMailbox,
    searchState: props.emptySearch,
    selectedMailboxThreadId: null,
    selectedSearchThreadId: null,
    thread: null,
    rulesState: { rules: [] },
    accountsState: { accounts: [] },
    diagnosticsState: null,
  });

  return {
    bridge: state.bridge,
    setBridge: (updater: SetStateAction<BridgeState>) => updateField(dispatch, "bridge", updater),
    externalPath: state.externalPath,
    setExternalPath: (updater: SetStateAction<string>) =>
      updateField(dispatch, "externalPath", updater),
    screen: state.screen,
    setScreen: (updater: SetStateAction<WorkbenchScreen>) =>
      updateField(dispatch, "screen", updater),
    layoutMode: state.layoutMode,
    setLayoutMode: (updater: SetStateAction<LayoutMode>) =>
      updateField(dispatch, "layoutMode", updater),
    focusContext: state.focusContext,
    setFocusContext: (updater: SetStateAction<FocusContext>) =>
      updateField(dispatch, "focusContext", updater),
    readerMode: state.readerMode,
    setReaderMode: (updater: SetStateAction<ReaderMode>) =>
      updateField(dispatch, "readerMode", updater),
    shell: state.shell,
    setShell: (updater: SetStateAction<WorkbenchShellPayload>) =>
      updateField(dispatch, "shell", updater),
    sidebar: state.sidebar,
    setSidebar: (updater: SetStateAction<SidebarPayload>) =>
      updateField(dispatch, "sidebar", updater),
    mailbox: state.mailbox,
    setMailbox: (updater: SetStateAction<MailboxPayload>) =>
      updateField(dispatch, "mailbox", updater),
    searchState: state.searchState,
    setSearchState: (updater: SetStateAction<SearchResponse>) =>
      updateField(dispatch, "searchState", updater),
    selectedMailboxThreadId: state.selectedMailboxThreadId,
    setSelectedMailboxThreadId: (updater: SetStateAction<string | null>) =>
      updateField(dispatch, "selectedMailboxThreadId", updater),
    selectedSearchThreadId: state.selectedSearchThreadId,
    setSelectedSearchThreadId: (updater: SetStateAction<string | null>) =>
      updateField(dispatch, "selectedSearchThreadId", updater),
    thread: state.thread,
    setThread: (updater: SetStateAction<ThreadResponse | null>) =>
      updateField(dispatch, "thread", updater),
    rulesState: state.rulesState,
    setRulesState: (updater: SetStateAction<RulesResponse>) =>
      updateField(dispatch, "rulesState", updater),
    accountsState: state.accountsState,
    setAccountsState: (updater: SetStateAction<AccountsResponse>) =>
      updateField(dispatch, "accountsState", updater),
    diagnosticsState: state.diagnosticsState,
    setDiagnosticsState: (updater: SetStateAction<DiagnosticsResponse | null>) =>
      updateField(dispatch, "diagnosticsState", updater),
  };
}
