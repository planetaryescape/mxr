import {
  AccountFormDialog,
  AttachmentDialog,
  GoToLabelDialog,
  LabelDialog,
  LinksDialog,
  MoveDialog,
  ReportDialog,
  RuleFormDialog,
  SavedSearchDialog,
  SnoozeDialog,
  UnsubscribeDialog,
} from "../dialogs";
import type {
  AccountOperationResponse,
  ComposeFrontmatter,
  ComposeSession,
  RuleFormPayload,
  SidebarItem,
  SnoozePreset,
  UtilityRailPayload,
} from "../../shared/types";
import { ComposeDialog } from "./ComposeDialog";

export function DesktopDialogs(props: {
  screen: "mailbox" | "search" | "rules" | "accounts" | "diagnostics";
  selectedRowSender: string | null;
  composeOpen: boolean;
  composeSession: ComposeSession | null;
  composeDraft: ComposeFrontmatter | null;
  composeBusy: string | null;
  composeError: string | null;
  utilityRail: UtilityRailPayload;
  onComposeDraftChange: (
    draft:
      | ComposeFrontmatter
      | null
      | ((current: ComposeFrontmatter | null) => ComposeFrontmatter | null),
  ) => void;
  onCloseCompose: () => void;
  onOpenComposeEditor: () => void;
  onRefreshCompose: () => void;
  onSendCompose: () => void;
  onSaveCompose: () => void;
  onDiscardCompose: () => void;
  onPersistComposeDraft: () => Promise<void>;
  onComposeBodyChange: (body: string) => void;
  fetchContactSuggestions: (query: string) => Promise<Array<{ label: string; value: string }>>;
  knownSenders: Array<{ name: string; email: string }>;
  labelDialogOpen: boolean;
  labelOptions: string[];
  selectedLabels: string[];
  customLabel: string;
  onCloseLabelDialog: () => void;
  onToggleLabel: (label: string) => void;
  onCustomLabelChange: (value: string) => void;
  onSubmitLabels: () => void;
  moveDialogOpen: boolean;
  moveTargetLabel: string;
  onCloseMoveDialog: () => void;
  onMoveTargetChange: (value: string) => void;
  onSubmitMove: () => void;
  snoozeDialogOpen: boolean;
  snoozePresets: SnoozePreset[];
  selectedSnooze: string;
  onCloseSnoozeDialog: () => void;
  onSelectedSnoozeChange: (value: string) => void;
  onSubmitSnooze: () => void;
  unsubscribeDialogOpen: boolean;
  onCloseUnsubscribeDialog: () => void;
  onSubmitUnsubscribe: () => void;
  goToLabelOpen: boolean;
  jumpLabelOptions: SidebarItem[];
  jumpTargetLabel: string;
  onCloseGoToLabelDialog: () => void;
  onJumpTargetLabelChange: (value: string) => void;
  onSubmitJumpTarget: () => void;
  attachmentDialogOpen: boolean;
  threadAttachments: Array<{
    id: string;
    filename: string;
    size_bytes: number;
    message_id: string;
  }>;
  onCloseAttachmentDialog: () => void;
  onOpenAttachment: (attachmentId: string, messageId: string) => void;
  onDownloadAttachment: (attachmentId: string, messageId: string) => void;
  linksDialogOpen: boolean;
  threadLinks: string[];
  onCloseLinksDialog: () => void;
  onOpenLink: (url: string) => void;
  reportOpen: boolean;
  reportTitle: string;
  reportContent: string;
  onCloseReportDialog: () => void;
  ruleFormOpen: boolean;
  ruleFormBusy: string | null;
  ruleFormState: RuleFormPayload;
  onCloseRuleFormDialog: () => void;
  onRuleFormChange: (
    value: RuleFormPayload | ((current: RuleFormPayload) => RuleFormPayload),
  ) => void;
  onSubmitRuleForm: () => void;
  accountFormOpen: boolean;
  accountFormBusy: string | null;
  accountDraftJson: string;
  accountResult: AccountOperationResponse["result"] | null;
  onCloseAccountFormDialog: () => void;
  onAccountDraftChange: (value: string) => void;
  onTestAccount: () => void;
  onSaveAccount: () => void;
  savedSearchDialogOpen: boolean;
  savedSearchName: string;
  savedSearchQuery: string;
  savedSearchMode: string;
  onCloseSavedSearchDialog: () => void;
  onSavedSearchNameChange: (value: string) => void;
  onSubmitSavedSearch: () => void;
}) {
  return (
    <>
      <ComposeDialog
        open={props.composeOpen}
        session={props.composeSession}
        draft={props.composeDraft}
        busyLabel={props.composeBusy}
        error={props.composeError}
        utilityRail={props.utilityRail}
        onDraftChange={props.onComposeDraftChange}
        onClose={props.onCloseCompose}
        onOpenEditor={props.onOpenComposeEditor}
        onRefresh={props.onRefreshCompose}
        onSend={props.onSendCompose}
        onSave={props.onSaveCompose}
        onDiscard={props.onDiscardCompose}
        onPersistDraft={props.onPersistComposeDraft}
        onBodyChange={props.onComposeBodyChange}
        fetchContactSuggestions={props.fetchContactSuggestions}
        knownSenders={props.knownSenders}
      />

      <LabelDialog
        open={props.labelDialogOpen}
        options={props.labelOptions}
        selected={props.selectedLabels}
        customLabel={props.customLabel}
        onClose={props.onCloseLabelDialog}
        onToggle={props.onToggleLabel}
        onCustomLabelChange={props.onCustomLabelChange}
        onSubmit={props.onSubmitLabels}
      />

      <MoveDialog
        open={props.moveDialogOpen}
        options={props.labelOptions}
        value={props.moveTargetLabel}
        onClose={props.onCloseMoveDialog}
        onValueChange={props.onMoveTargetChange}
        onSubmit={props.onSubmitMove}
      />

      <SnoozeDialog
        open={props.snoozeDialogOpen}
        presets={props.snoozePresets}
        value={props.selectedSnooze}
        onClose={props.onCloseSnoozeDialog}
        onValueChange={props.onSelectedSnoozeChange}
        onSubmit={props.onSubmitSnooze}
      />

      <UnsubscribeDialog
        open={props.unsubscribeDialogOpen}
        sender={props.selectedRowSender ?? "sender"}
        onClose={props.onCloseUnsubscribeDialog}
        onSubmit={props.onSubmitUnsubscribe}
      />

      <GoToLabelDialog
        open={props.goToLabelOpen}
        options={props.jumpLabelOptions}
        value={props.jumpTargetLabel}
        onClose={props.onCloseGoToLabelDialog}
        onValueChange={props.onJumpTargetLabelChange}
        onSubmit={props.onSubmitJumpTarget}
      />

      <AttachmentDialog
        open={props.attachmentDialogOpen}
        attachments={props.threadAttachments}
        onClose={props.onCloseAttachmentDialog}
        onOpen={props.onOpenAttachment}
        onDownload={props.onDownloadAttachment}
      />

      <LinksDialog
        open={props.linksDialogOpen}
        links={props.threadLinks}
        onClose={props.onCloseLinksDialog}
        onOpen={props.onOpenLink}
      />

      <ReportDialog
        open={props.reportOpen}
        title={props.reportTitle}
        content={props.reportContent}
        onClose={props.onCloseReportDialog}
      />

      <RuleFormDialog
        open={props.ruleFormOpen}
        busyLabel={props.ruleFormBusy}
        form={props.ruleFormState}
        onClose={props.onCloseRuleFormDialog}
        onChange={props.onRuleFormChange}
        onSubmit={props.onSubmitRuleForm}
      />

      <SavedSearchDialog
        open={props.savedSearchDialogOpen}
        name={props.savedSearchName}
        query={props.savedSearchQuery}
        mode={props.savedSearchMode}
        onClose={props.onCloseSavedSearchDialog}
        onNameChange={props.onSavedSearchNameChange}
        onSubmit={props.onSubmitSavedSearch}
      />

      <AccountFormDialog
        open={props.accountFormOpen}
        busyLabel={props.accountFormBusy}
        draftJson={props.accountDraftJson}
        result={props.accountResult}
        onClose={props.onCloseAccountFormDialog}
        onChange={props.onAccountDraftChange}
        onTest={props.onTestAccount}
        onSave={props.onSaveAccount}
      />
    </>
  );
}
