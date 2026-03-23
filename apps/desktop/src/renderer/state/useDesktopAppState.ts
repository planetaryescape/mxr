import { useCallback, useReducer } from "react";
import type { SetStateAction } from "react";
import type {
  AccountOperationResponse,
  ComposeFrontmatter,
  ComposeSession,
  RuleFormPayload,
  SearchMode,
  SearchScope,
  SearchSort,
  SnoozePreset,
} from "../../shared/types";
import type { PendingBinding } from "../lib/tui-manifest";
import { objectStateReducer, updateField } from "./objectState";

export type PendingMutationState = {
  messageIds: Set<string>;
  label: string;
};

type SearchControlsState = {
  searchQuery: string;
  searchScope: SearchScope;
  searchMode: SearchMode;
  searchSort: SearchSort;
  searchExplain: boolean;
};

type UiChromeState = {
  pendingBinding: PendingBinding | null;
  commandPaletteOpen: boolean;
  commandQuery: string;
  helpOpen: boolean;
  actionNotice: string | null;
  pendingMutation: PendingMutationState | null;
  showInboxZero: boolean;
  workbenchReady: boolean;
  mailListMode: "threads" | "messages";
  signatureExpanded: boolean;
  selectedMessageIds: Set<string>;
  visualMode: boolean;
  visualAnchorMessageId: string | null;
};

type ComposeState = {
  composeSession: ComposeSession | null;
  composeOpen: boolean;
  composeDraft: ComposeFrontmatter | null;
  composeBusy: string | null;
  composeError: string | null;
};

type DialogState = {
  labelDialogOpen: boolean;
  selectedLabels: string[];
  customLabel: string;
  moveDialogOpen: boolean;
  moveTargetLabel: string;
  snoozeDialogOpen: boolean;
  snoozePresets: SnoozePreset[];
  selectedSnooze: string;
  unsubscribeDialogOpen: boolean;
  goToLabelOpen: boolean;
  jumpTargetLabel: string;
  attachmentDialogOpen: boolean;
  linksDialogOpen: boolean;
  reportOpen: boolean;
  reportTitle: string;
  reportContent: string;
};

type RulesState = {
  selectedRuleId: string | null;
  ruleDetail: Record<string, unknown> | null;
  rulePanelMode: "details" | "history" | "dryRun";
  ruleHistoryState: Array<Record<string, unknown>>;
  ruleDryRunState: Array<Record<string, unknown>>;
  ruleStatus: string | null;
  ruleFormOpen: boolean;
  ruleFormBusy: string | null;
  ruleFormState: RuleFormPayload;
};

type AccountsState = {
  selectedAccountId: string | null;
  accountStatus: string | null;
  accountResult: AccountOperationResponse["result"] | null;
  accountFormOpen: boolean;
  accountFormBusy: string | null;
  accountDraftJson: string;
};

const INITIAL_SEARCH_CONTROLS: SearchControlsState = {
  searchQuery: "",
  searchScope: "threads",
  searchMode: "lexical",
  searchSort: "relevant",
  searchExplain: false,
};

const INITIAL_UI_CHROME: UiChromeState = {
  pendingBinding: null,
  commandPaletteOpen: false,
  commandQuery: "",
  helpOpen: false,
  actionNotice: null,
  pendingMutation: null,
  showInboxZero: false,
  workbenchReady: false,
  mailListMode: "threads",
  signatureExpanded: false,
  selectedMessageIds: new Set(),
  visualMode: false,
  visualAnchorMessageId: null,
};

const INITIAL_COMPOSE_STATE: ComposeState = {
  composeSession: null,
  composeOpen: false,
  composeDraft: null,
  composeBusy: null,
  composeError: null,
};

const INITIAL_DIALOG_STATE: DialogState = {
  labelDialogOpen: false,
  selectedLabels: [],
  customLabel: "",
  moveDialogOpen: false,
  moveTargetLabel: "",
  snoozeDialogOpen: false,
  snoozePresets: [],
  selectedSnooze: "",
  unsubscribeDialogOpen: false,
  goToLabelOpen: false,
  jumpTargetLabel: "",
  attachmentDialogOpen: false,
  linksDialogOpen: false,
  reportOpen: false,
  reportTitle: "",
  reportContent: "",
};

const INITIAL_RULES_STATE: RulesState = {
  selectedRuleId: null,
  ruleDetail: null,
  rulePanelMode: "details",
  ruleHistoryState: [],
  ruleDryRunState: [],
  ruleStatus: null,
  ruleFormOpen: false,
  ruleFormBusy: null,
  ruleFormState: {
    id: null,
    name: "",
    condition: "",
    action: "",
    priority: 100,
    enabled: true,
  },
};

const INITIAL_ACCOUNTS_STATE: AccountsState = {
  selectedAccountId: null,
  accountStatus: null,
  accountResult: null,
  accountFormOpen: false,
  accountFormBusy: null,
  accountDraftJson: "",
};

export function useDesktopAppState() {
  const [searchControls, dispatchSearchControls] = useReducer(
    objectStateReducer<SearchControlsState>,
    INITIAL_SEARCH_CONTROLS,
  );
  const [uiChrome, dispatchUiChrome] = useReducer(
    objectStateReducer<UiChromeState>,
    INITIAL_UI_CHROME,
  );
  const [composeState, dispatchComposeState] = useReducer(
    objectStateReducer<ComposeState>,
    INITIAL_COMPOSE_STATE,
  );
  const [dialogState, dispatchDialogState] = useReducer(
    objectStateReducer<DialogState>,
    INITIAL_DIALOG_STATE,
  );
  const [rulesState, dispatchRulesState] = useReducer(
    objectStateReducer<RulesState>,
    INITIAL_RULES_STATE,
  );
  const [accountsState, dispatchAccountsState] = useReducer(
    objectStateReducer<AccountsState>,
    INITIAL_ACCOUNTS_STATE,
  );

  const modalOpen =
    composeState.composeOpen ||
    dialogState.labelDialogOpen ||
    dialogState.moveDialogOpen ||
    dialogState.snoozeDialogOpen ||
    dialogState.unsubscribeDialogOpen ||
    dialogState.goToLabelOpen ||
    dialogState.attachmentDialogOpen ||
    dialogState.linksDialogOpen ||
    dialogState.reportOpen ||
    rulesState.ruleFormOpen ||
    accountsState.accountFormOpen;

  const closeAllDialogs = useCallback(() => {
    dispatchDialogState({
      type: "patch",
      patch: {
        labelDialogOpen: false,
        moveDialogOpen: false,
        snoozeDialogOpen: false,
        unsubscribeDialogOpen: false,
        goToLabelOpen: false,
        attachmentDialogOpen: false,
        linksDialogOpen: false,
        reportOpen: false,
      },
    });
    updateField(dispatchRulesState, "ruleFormOpen", false);
    updateField(dispatchAccountsState, "accountFormOpen", false);
  }, []);

  return {
    searchQuery: searchControls.searchQuery,
    setSearchQuery: (updater: SetStateAction<string>) =>
      updateField(dispatchSearchControls, "searchQuery", updater),
    searchScope: searchControls.searchScope,
    setSearchScope: (updater: SetStateAction<SearchScope>) =>
      updateField(dispatchSearchControls, "searchScope", updater),
    searchMode: searchControls.searchMode,
    setSearchMode: (updater: SetStateAction<SearchMode>) =>
      updateField(dispatchSearchControls, "searchMode", updater),
    searchSort: searchControls.searchSort,
    setSearchSort: (updater: SetStateAction<SearchSort>) =>
      updateField(dispatchSearchControls, "searchSort", updater),
    searchExplain: searchControls.searchExplain,
    setSearchExplain: (updater: SetStateAction<boolean>) =>
      updateField(dispatchSearchControls, "searchExplain", updater),

    pendingBinding: uiChrome.pendingBinding,
    setPendingBinding: (updater: SetStateAction<PendingBinding | null>) =>
      updateField(dispatchUiChrome, "pendingBinding", updater),
    commandPaletteOpen: uiChrome.commandPaletteOpen,
    setCommandPaletteOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchUiChrome, "commandPaletteOpen", updater),
    commandQuery: uiChrome.commandQuery,
    setCommandQuery: (updater: SetStateAction<string>) =>
      updateField(dispatchUiChrome, "commandQuery", updater),
    helpOpen: uiChrome.helpOpen,
    setHelpOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchUiChrome, "helpOpen", updater),
    actionNotice: uiChrome.actionNotice,
    setActionNotice: (updater: SetStateAction<string | null>) =>
      updateField(dispatchUiChrome, "actionNotice", updater),
    pendingMutation: uiChrome.pendingMutation,
    setPendingMutation: (updater: SetStateAction<PendingMutationState | null>) =>
      updateField(dispatchUiChrome, "pendingMutation", updater),
    showInboxZero: uiChrome.showInboxZero,
    setShowInboxZero: (updater: SetStateAction<boolean>) =>
      updateField(dispatchUiChrome, "showInboxZero", updater),
    workbenchReady: uiChrome.workbenchReady,
    setWorkbenchReady: (updater: SetStateAction<boolean>) =>
      updateField(dispatchUiChrome, "workbenchReady", updater),
    mailListMode: uiChrome.mailListMode,
    setMailListMode: (updater: SetStateAction<"threads" | "messages">) =>
      updateField(dispatchUiChrome, "mailListMode", updater),
    signatureExpanded: uiChrome.signatureExpanded,
    setSignatureExpanded: (updater: SetStateAction<boolean>) =>
      updateField(dispatchUiChrome, "signatureExpanded", updater),
    selectedMessageIds: uiChrome.selectedMessageIds,
    setSelectedMessageIds: (updater: SetStateAction<Set<string>>) =>
      updateField(dispatchUiChrome, "selectedMessageIds", updater),
    visualMode: uiChrome.visualMode,
    setVisualMode: (updater: SetStateAction<boolean>) =>
      updateField(dispatchUiChrome, "visualMode", updater),
    visualAnchorMessageId: uiChrome.visualAnchorMessageId,
    setVisualAnchorMessageId: (updater: SetStateAction<string | null>) =>
      updateField(dispatchUiChrome, "visualAnchorMessageId", updater),

    composeSession: composeState.composeSession,
    setComposeSession: (updater: SetStateAction<ComposeSession | null>) =>
      updateField(dispatchComposeState, "composeSession", updater),
    composeOpen: composeState.composeOpen,
    setComposeOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchComposeState, "composeOpen", updater),
    composeDraft: composeState.composeDraft,
    setComposeDraft: (updater: SetStateAction<ComposeFrontmatter | null>) =>
      updateField(dispatchComposeState, "composeDraft", updater),
    composeBusy: composeState.composeBusy,
    setComposeBusy: (updater: SetStateAction<string | null>) =>
      updateField(dispatchComposeState, "composeBusy", updater),
    composeError: composeState.composeError,
    setComposeError: (updater: SetStateAction<string | null>) =>
      updateField(dispatchComposeState, "composeError", updater),

    labelDialogOpen: dialogState.labelDialogOpen,
    setLabelDialogOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchDialogState, "labelDialogOpen", updater),
    selectedLabels: dialogState.selectedLabels,
    setSelectedLabels: (updater: SetStateAction<string[]>) =>
      updateField(dispatchDialogState, "selectedLabels", updater),
    customLabel: dialogState.customLabel,
    setCustomLabel: (updater: SetStateAction<string>) =>
      updateField(dispatchDialogState, "customLabel", updater),
    moveDialogOpen: dialogState.moveDialogOpen,
    setMoveDialogOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchDialogState, "moveDialogOpen", updater),
    moveTargetLabel: dialogState.moveTargetLabel,
    setMoveTargetLabel: (updater: SetStateAction<string>) =>
      updateField(dispatchDialogState, "moveTargetLabel", updater),
    snoozeDialogOpen: dialogState.snoozeDialogOpen,
    setSnoozeDialogOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchDialogState, "snoozeDialogOpen", updater),
    snoozePresets: dialogState.snoozePresets,
    setSnoozePresets: (updater: SetStateAction<SnoozePreset[]>) =>
      updateField(dispatchDialogState, "snoozePresets", updater),
    selectedSnooze: dialogState.selectedSnooze,
    setSelectedSnooze: (updater: SetStateAction<string>) =>
      updateField(dispatchDialogState, "selectedSnooze", updater),
    unsubscribeDialogOpen: dialogState.unsubscribeDialogOpen,
    setUnsubscribeDialogOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchDialogState, "unsubscribeDialogOpen", updater),
    goToLabelOpen: dialogState.goToLabelOpen,
    setGoToLabelOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchDialogState, "goToLabelOpen", updater),
    jumpTargetLabel: dialogState.jumpTargetLabel,
    setJumpTargetLabel: (updater: SetStateAction<string>) =>
      updateField(dispatchDialogState, "jumpTargetLabel", updater),
    attachmentDialogOpen: dialogState.attachmentDialogOpen,
    setAttachmentDialogOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchDialogState, "attachmentDialogOpen", updater),
    linksDialogOpen: dialogState.linksDialogOpen,
    setLinksDialogOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchDialogState, "linksDialogOpen", updater),
    reportOpen: dialogState.reportOpen,
    setReportOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchDialogState, "reportOpen", updater),
    reportTitle: dialogState.reportTitle,
    setReportTitle: (updater: SetStateAction<string>) =>
      updateField(dispatchDialogState, "reportTitle", updater),
    reportContent: dialogState.reportContent,
    setReportContent: (updater: SetStateAction<string>) =>
      updateField(dispatchDialogState, "reportContent", updater),

    selectedRuleId: rulesState.selectedRuleId,
    setSelectedRuleId: (updater: SetStateAction<string | null>) =>
      updateField(dispatchRulesState, "selectedRuleId", updater),
    ruleDetail: rulesState.ruleDetail,
    setRuleDetail: (updater: SetStateAction<Record<string, unknown> | null>) =>
      updateField(dispatchRulesState, "ruleDetail", updater),
    rulePanelMode: rulesState.rulePanelMode,
    setRulePanelMode: (updater: SetStateAction<"details" | "history" | "dryRun">) =>
      updateField(dispatchRulesState, "rulePanelMode", updater),
    ruleHistoryState: rulesState.ruleHistoryState,
    setRuleHistoryState: (updater: SetStateAction<Array<Record<string, unknown>>>) =>
      updateField(dispatchRulesState, "ruleHistoryState", updater),
    ruleDryRunState: rulesState.ruleDryRunState,
    setRuleDryRunState: (updater: SetStateAction<Array<Record<string, unknown>>>) =>
      updateField(dispatchRulesState, "ruleDryRunState", updater),
    ruleStatus: rulesState.ruleStatus,
    setRuleStatus: (updater: SetStateAction<string | null>) =>
      updateField(dispatchRulesState, "ruleStatus", updater),
    ruleFormOpen: rulesState.ruleFormOpen,
    setRuleFormOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchRulesState, "ruleFormOpen", updater),
    ruleFormBusy: rulesState.ruleFormBusy,
    setRuleFormBusy: (updater: SetStateAction<string | null>) =>
      updateField(dispatchRulesState, "ruleFormBusy", updater),
    ruleFormState: rulesState.ruleFormState,
    setRuleFormState: (updater: SetStateAction<RuleFormPayload>) =>
      updateField(dispatchRulesState, "ruleFormState", updater),

    selectedAccountId: accountsState.selectedAccountId,
    setSelectedAccountId: (updater: SetStateAction<string | null>) =>
      updateField(dispatchAccountsState, "selectedAccountId", updater),
    accountStatus: accountsState.accountStatus,
    setAccountStatus: (updater: SetStateAction<string | null>) =>
      updateField(dispatchAccountsState, "accountStatus", updater),
    accountResult: accountsState.accountResult,
    setAccountResult: (updater: SetStateAction<AccountOperationResponse["result"] | null>) =>
      updateField(dispatchAccountsState, "accountResult", updater),
    accountFormOpen: accountsState.accountFormOpen,
    setAccountFormOpen: (updater: SetStateAction<boolean>) =>
      updateField(dispatchAccountsState, "accountFormOpen", updater),
    accountFormBusy: accountsState.accountFormBusy,
    setAccountFormBusy: (updater: SetStateAction<string | null>) =>
      updateField(dispatchAccountsState, "accountFormBusy", updater),
    accountDraftJson: accountsState.accountDraftJson,
    setAccountDraftJson: (updater: SetStateAction<string>) =>
      updateField(dispatchAccountsState, "accountDraftJson", updater),

    modalOpen,
    closeAllDialogs,
  };
}
