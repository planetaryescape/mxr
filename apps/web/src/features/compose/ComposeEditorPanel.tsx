import { FilePlus2, Loader2, Paperclip, Trash2, X } from "lucide-react";
import { lazy, Suspense, useRef, useState, type DragEvent } from "react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { ComposeActionBar } from "./ComposeActionBar";
import { ComposeIssueSummary } from "./ComposeIssueSummary";
import { ComposeTopBar } from "./ComposeTopBar";
import { DraftAssist } from "./DraftAssist";
import { RecipientField } from "./RecipientField";
import { SendConfirmDialog } from "./SendConfirmDialog";
import { SendLaterDialog } from "./SendLaterDialog";
import { SignaturePicker } from "./SignaturePicker";
import { SnippetPicker } from "./SnippetPicker";
import type { ComposeController, ComposeUploadProgress } from "./useComposeSession";

const CodeMirrorComposeEditor = lazy(() =>
  import("./codemirror/CodeMirrorComposeEditor").then((module) => ({
    default: module.CodeMirrorComposeEditor,
  })),
);
const TiptapComposeEditor = lazy(() =>
  import("./tiptap/TiptapComposeEditor").then((module) => ({
    default: module.TiptapComposeEditor,
  })),
);

const REMIND_DIALOG_PRESETS = [
  { label: "Tomorrow 9am", input: "tomorrow 9am" },
  { label: "In 3 days", input: "in 3 days" },
  { label: "Next monday 9am", input: "next monday 9am" },
] as const;

export function ComposeEditorPanel({ controller }: { controller: ComposeController }) {
  const editorPreference = useUiPrefs((state) => state.composeEditor);
  const setComposeEditor = useUiPrefs((state) => state.setComposeEditor);

  const [dragActive, setDragActive] = useState(false);
  const dragDepth = useRef(0);

  const { draft } = controller;
  if (!draft) return null;

  function onDragEnter(event: DragEvent<HTMLDivElement>) {
    if (!hasFiles(event.dataTransfer)) return;
    event.preventDefault();
    dragDepth.current += 1;
    setDragActive(true);
  }

  function onDragOver(event: DragEvent<HTMLDivElement>) {
    if (!hasFiles(event.dataTransfer)) return;
    event.preventDefault();
  }

  function onDragLeave(event: DragEvent<HTMLDivElement>) {
    if (!hasFiles(event.dataTransfer)) return;
    dragDepth.current = Math.max(0, dragDepth.current - 1);
    if (dragDepth.current === 0) setDragActive(false);
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    if (!hasFiles(event.dataTransfer)) return;
    event.preventDefault();
    dragDepth.current = 0;
    setDragActive(false);
    void controller.addFiles(event.dataTransfer.files);
  }

  return (
    <div
      className="flex min-w-0 flex-1 flex-col overflow-hidden bg-background"
      onKeyDown={controller.handleComposeKeyDown}
    >
      <ComposeTopBar
        title={controller.intent.title}
        busy={controller.busy}
        canServerSave={controller.canServerSave}
        onRefresh={controller.handleRefreshClick}
        onServerSave={controller.handleServerSaveClick}
        onDiscard={controller.requestDiscard}
        accounts={controller.runtimeAccounts}
        accountId={draft.accountId}
        onAccountChange={controller.updateAccount}
        addresses={controller.accountAddresses}
        fromAddress={draft.frontmatter.from}
        onFromChange={(email) => controller.updateFrontmatter("from", email)}
      />

      <div className="shrink-0 border-b border-border">
        <div className="mx-auto w-full max-w-[860px] px-4 py-1.5">
          {controller.collaboratorSuggestions.length > 0 ? (
            <div className="flex flex-wrap items-center gap-1.5 px-1 pb-1 text-2xs text-muted-foreground">
              <span>Maybe cc:</span>
              {controller.collaboratorSuggestions.map((suggestion) => (
                <button
                  key={suggestion.email}
                  type="button"
                  className="rounded-full border border-border bg-muted/40 px-2 py-0.5 text-2xs text-foreground transition-colors hover:border-primary/50 hover:bg-muted"
                  title={suggestion.reason}
                  aria-label={`Add ${suggestion.email} to Cc`}
                  onClick={() => controller.addCc(suggestion.email)}
                >
                  {suggestion.display_name || suggestion.email}
                </button>
              ))}
            </div>
          ) : null}
          <RecipientField
            label="To"
            value={draft.frontmatter.to}
            inputRef={controller.toInputRef}
            onChange={(value) => controller.updateFrontmatter("to", value)}
            onBlur={controller.markRecipientsTouched}
            trailing={
              <>
                {!controller.showCc ? (
                  <Button
                    variant="ghost"
                    size="xs"
                    onClick={controller.revealCc}
                    title="Add Cc (⇧⌘C)"
                  >
                    Cc
                  </Button>
                ) : null}
                {!controller.showBcc ? (
                  <Button
                    variant="ghost"
                    size="xs"
                    onClick={controller.revealBcc}
                    title="Add Bcc (⇧⌘B)"
                  >
                    Bcc
                  </Button>
                ) : null}
              </>
            }
          />
          <Collapsible open={controller.showCc} onOpenChange={controller.setShowCc}>
            <CollapsibleContent>
              <RecipientField
                label="Cc"
                value={draft.frontmatter.cc}
                inputRef={controller.ccInputRef}
                onChange={(value) => controller.updateFrontmatter("cc", value)}
                onBlur={controller.markRecipientsTouched}
              />
            </CollapsibleContent>
          </Collapsible>
          <Collapsible open={controller.showBcc} onOpenChange={controller.setShowBcc}>
            <CollapsibleContent>
              <RecipientField
                label="Bcc"
                value={draft.frontmatter.bcc}
                inputRef={controller.bccInputRef}
                onChange={(value) => controller.updateFrontmatter("bcc", value)}
                onBlur={controller.markRecipientsTouched}
              />
            </CollapsibleContent>
          </Collapsible>
          <div className="mt-1 grid grid-cols-[3.25rem_minmax(0,1fr)] items-center gap-3 border-t border-border/60 px-1 pt-1.5">
            <Label
              htmlFor="compose-subject"
              className="text-right text-xs font-medium text-muted-foreground"
            >
              Subject
            </Label>
            <Input
              id="compose-subject"
              value={draft.frontmatter.subject}
              onChange={(event) => controller.updateFrontmatter("subject", event.target.value)}
              placeholder="Subject"
              className="h-9 bg-input text-md font-medium"
            />
          </div>
        </div>
      </div>

      <DraftAssist
        open={controller.assistOpen}
        onOpenChange={controller.setAssistOpen}
        purpose={controller.aiPurpose}
        onPurposeChange={controller.setAiPurpose}
        register={controller.aiRegister}
        onRegisterChange={controller.onRegisterChange}
        length={controller.aiLength}
        onLengthChange={controller.onLengthChange}
        overridden={controller.aiOverridden}
        onResetTone={controller.resetTone}
        contextNote={controller.draftSuggestion?.context_note ?? null}
        onGenerate={controller.generateDraft}
        generating={controller.generating}
        refineContext={controller.refineContext}
        onRefineContextChange={controller.setRefineContext}
        onRefine={controller.runRefine}
        refining={controller.refining}
        canRefine={controller.canRefine}
        suggestion={controller.draftSuggestion}
        busy={controller.busy}
      />

      <div
        role="group"
        aria-label="Message body"
        className="relative min-h-0 flex-1 overflow-hidden"
        onDragEnter={onDragEnter}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
      >
        <div className="mx-auto h-full w-full max-w-[860px]">
          <Suspense
            fallback={
              <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
                Loading editor…
              </div>
            }
          >
            {editorPreference === "tiptap" ? (
              <TiptapComposeEditor
                value={draft.bodyMarkdown}
                onChange={controller.updateBody}
                onSave={controller.handleSaveClick}
                onSend={controller.requestSend}
                onDiscard={controller.requestDiscard}
              />
            ) : (
              <CodeMirrorComposeEditor
                value={draft.bodyMarkdown}
                onChange={controller.updateBody}
                onSave={controller.handleSaveClick}
                onSend={controller.requestSend}
                onDiscard={controller.requestDiscard}
                onClose={() => void controller.requestClose()}
              />
            )}
          </Suspense>
        </div>
        {dragActive ? (
          <div className="absolute inset-0 z-20 flex items-center justify-center bg-background/80 backdrop-blur-sm">
            <div className="border border-primary bg-popover px-5 py-4 text-center shadow-xl">
              <FilePlus2 className="mx-auto mb-2 size-6 text-primary" />
              <div className="text-sm font-medium">Drop files to attach</div>
              <div className="mt-1 text-xs text-muted-foreground">
                mxr stores a local copy for this draft.
              </div>
            </div>
          </div>
        ) : null}
      </div>

      {draft.frontmatter.attach.length > 0 ||
      controller.uploadProgress.length > 0 ||
      controller.visibleIssues.length > 0 ? (
        <div className="shrink-0 border-t border-border">
          <div className="mx-auto w-full max-w-[860px] space-y-2 px-5 py-2">
            {draft.frontmatter.attach.length > 0 || controller.uploadProgress.length > 0 ? (
              <AttachmentList
                attachments={draft.frontmatter.attach}
                uploads={controller.uploadProgress}
                onRemove={controller.removeAttachment}
              />
            ) : null}
            <ComposeIssueSummary issues={controller.visibleIssues} />
          </div>
        </div>
      ) : null}

      <ComposeActionBar
        onSend={controller.requestSend}
        onSendLater={controller.requestSendLater}
        onSendAndArchive={
          controller.intent.messageId ? controller.requestSendAndArchive : undefined
        }
        onSendAndRemind={controller.requestSendAndRemind}
        onSendAndRemindCustom={() => controller.setRemindDialogOpen(true)}
        onAttach={controller.handleAttachShortcut}
        uploading={controller.uploading}
        busy={controller.busy}
        saveStatus={controller.saveStatus}
        dirty={controller.dirty}
        saveError={controller.saveError}
        onRetrySave={controller.retrySave}
        editorPreference={editorPreference}
        onEditorChange={setComposeEditor}
        suggestion={controller.draftSuggestion}
      />

      <input
        ref={controller.fileInputRef}
        type="file"
        multiple
        aria-label="Attach files"
        tabIndex={-1}
        className="sr-only"
        onChange={(event) => {
          if (event.currentTarget.files) void controller.addFiles(event.currentTarget.files);
          event.currentTarget.value = "";
        }}
      />

      <SendConfirmDialog
        open={controller.sendConfirmOpen}
        onOpenChange={controller.setSendConfirmOpen}
        frontmatter={draft.frontmatter}
        fromAddress={
          draft.frontmatter.from || controller.selectedAccount?.email || "the selected account"
        }
        suggestion={controller.draftSuggestion}
        sending={controller.sending}
        safetyReport={controller.safetyReport}
        safetyCheckError={controller.safetyCheckError}
        collaborators={controller.collaboratorSuggestions}
        onAddCc={controller.addCc}
        onConfirm={(override) => void controller.confirmSend(override)}
      />
      <SnippetPicker
        open={controller.snippetPickerOpen}
        onOpenChange={controller.setSnippetPickerOpen}
        snippets={controller.snippetList}
        onInsert={controller.insertSnippet}
      />
      <SignaturePicker
        open={controller.signaturePickerOpen}
        onOpenChange={controller.setSignaturePickerOpen}
        signatures={controller.signatureList}
        onInsert={controller.insertSignature}
      />
      <SendLaterDialog
        open={controller.sendLaterOpen}
        onOpenChange={controller.setSendLaterOpen}
        scheduling={controller.scheduling}
        onConfirm={(at, label) => controller.scheduleSend(at, label)}
      />
      <SendLaterDialog
        open={controller.remindDialogOpen}
        onOpenChange={controller.setRemindDialogOpen}
        scheduling={controller.busy}
        onConfirm={controller.requestSendAndRemind}
        title="Send and remind me"
        description="Send now. If nobody replies by this time, mxr reminds you."
        confirmLabel="Send and set reminder"
        presets={REMIND_DIALOG_PRESETS}
      />
      <DiscardConfirmDialog
        open={controller.discardConfirmOpen}
        onOpenChange={controller.setDiscardConfirmOpen}
        discarding={controller.discarding}
        onConfirm={controller.discardDraft}
      />
    </div>
  );
}

function AttachmentList({
  attachments,
  uploads,
  onRemove,
}: {
  attachments: string[];
  uploads: ComposeUploadProgress[];
  onRemove: (path: string) => void;
}) {
  if (attachments.length === 0 && uploads.length === 0) {
    return <div className="text-2xs text-muted-foreground">No attachments</div>;
  }
  return (
    <div className="flex flex-1 flex-wrap gap-1.5">
      {uploads.map((upload) => (
        <Badge
          key={upload.id}
          variant="outline"
          className="max-w-full bg-background py-1 text-muted-foreground"
        >
          {upload.done ? (
            <Paperclip className="size-3" />
          ) : (
            <Loader2 className="size-3 animate-spin" />
          )}
          <span className="max-w-[220px] truncate" title={upload.name}>
            {upload.name}
          </span>
          <span className="text-2xs">{upload.done ? "uploaded" : "uploading…"}</span>
        </Badge>
      ))}
      {attachments.map((path) => (
        <Badge
          key={path}
          variant="outline"
          className="max-w-full bg-background py-1 text-foreground"
        >
          <Paperclip className="size-3 text-muted-foreground" />
          <span className="max-w-[220px] truncate" title={path}>
            {basename(path)}
          </span>
          <button
            type="button"
            className="text-muted-foreground hover:text-foreground"
            onClick={() => onRemove(path)}
            aria-label={`Remove ${basename(path)}`}
          >
            <X className="size-3" />
          </button>
        </Badge>
      ))}
    </div>
  );
}

function DiscardConfirmDialog({
  open,
  onOpenChange,
  discarding,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  discarding: boolean;
  onConfirm: () => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent
        onKeyDown={(event) => {
          if (event.key === "Enter" && !discarding) {
            event.preventDefault();
            onConfirm();
          }
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle>Discard draft?</AlertDialogTitle>
          <AlertDialogDescription>
            This deletes the local compose file and attachments for this draft.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel variant="outline" disabled={discarding}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={discarding}
            onClick={(event) => {
              event.preventDefault();
              onConfirm();
            }}
          >
            {discarding ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Trash2 className="size-3" />
            )}
            Discard
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function hasFiles(dataTransfer: DataTransfer): boolean {
  return Array.from(dataTransfer.types).includes("Files");
}

function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  for (let index = parts.length - 1; index >= 0; index -= 1) {
    const part = parts[index];
    if (part) return part;
  }
  return path;
}
