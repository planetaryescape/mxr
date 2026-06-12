import {
  AlertTriangle,
  FilePlus2,
  Loader2,
  Paperclip,
  Send,
  Trash2,
  X,
} from "lucide-react";
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
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useUiPrefs } from "@/state/uiPrefsStore";
import type { ComposeIssue, RuntimeAccount } from "./api";
import { ComposeActionBar } from "./ComposeActionBar";
import { ComposeTopBar } from "./ComposeTopBar";
import { DraftAssist } from "./DraftAssist";
import { DraftQualityBadges } from "./DraftQualityBadges";
import { RecipientField } from "./RecipientField";
import type { DraftSuggestionResponse } from "./types";
import type { ComposeController } from "./useComposeSession";

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
      />

      <div className="shrink-0 border-b border-border">
        <div className="mx-auto w-full max-w-[860px] px-4 py-1.5">
          <RecipientField
            label="To"
            value={draft.frontmatter.to}
            inputRef={controller.toInputRef}
            onChange={(value) => controller.updateFrontmatter("to", value)}
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

      {draft.frontmatter.attach.length > 0 || controller.visibleIssues.length > 0 ? (
        <div className="shrink-0 border-t border-border">
          <div className="mx-auto w-full max-w-[860px] space-y-2 px-5 py-2">
            {draft.frontmatter.attach.length > 0 ? (
              <AttachmentList
                attachments={draft.frontmatter.attach}
                onRemove={controller.removeAttachment}
              />
            ) : null}
            <IssueList issues={controller.visibleIssues} />
          </div>
        </div>
      ) : null}

      <ComposeActionBar
        onSend={controller.requestSend}
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
        className="sr-only"
        onChange={(event) => {
          if (event.currentTarget.files) void controller.addFiles(event.currentTarget.files);
          event.currentTarget.value = "";
        }}
      />

      <SendConfirmDialog
        open={controller.sendConfirmOpen}
        onOpenChange={controller.setSendConfirmOpen}
        recipientCount={controller.recipientCount}
        account={controller.selectedAccount}
        subject={draft.frontmatter.subject}
        suggestion={controller.draftSuggestion}
        sending={controller.sending}
        onConfirm={controller.confirmSend}
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
  onRemove,
}: {
  attachments: string[];
  onRemove: (path: string) => void;
}) {
  if (attachments.length === 0) {
    return <div className="text-2xs text-muted-foreground">No attachments</div>;
  }
  return (
    <div className="flex flex-1 flex-wrap gap-1.5">
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

function IssueList({ issues }: { issues: ComposeIssue[] }) {
  if (issues.length === 0) return null;
  return (
    <div className="mt-3 space-y-2">
      {issues.map((issue) => (
        <Alert
          key={`${issue.severity}-${issue.message}`}
          variant={issue.severity === "error" ? "destructive" : "warning"}
          className="flex items-center gap-2 px-3 py-2"
        >
          <AlertTriangle
            className={
              issue.severity === "error"
                ? "size-3 shrink-0 text-destructive"
                : "size-3 shrink-0 text-warning"
            }
          />
          <AlertDescription>{issue.message}</AlertDescription>
        </Alert>
      ))}
    </div>
  );
}

function SendConfirmDialog({
  open,
  onOpenChange,
  recipientCount,
  account,
  subject,
  suggestion,
  sending,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  recipientCount: number;
  account?: RuntimeAccount;
  subject: string;
  suggestion: DraftSuggestionResponse | null;
  sending: boolean;
  onConfirm: () => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent
        onKeyDown={(event) => {
          if (event.key === "Enter" && !sending) {
            event.preventDefault();
            onConfirm();
          }
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle>Send message?</AlertDialogTitle>
          <AlertDialogDescription>
            Send to {recipientCount} {recipientCount === 1 ? "recipient" : "recipients"} via{" "}
            {account?.email ?? "the selected account"}.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="rounded-lg border border-border bg-muted px-3 py-2 text-sm">
          {subject.trim() || "(no subject)"}
        </div>
        <DraftQualityBadges suggestion={suggestion} />
        <AlertDialogFooter>
          <AlertDialogCancel variant="outline" disabled={sending}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction
            disabled={sending}
            onClick={(event) => {
              event.preventDefault();
              onConfirm();
            }}
          >
            {sending ? <Loader2 className="size-3 animate-spin" /> : <Send className="size-3" />}
            Send
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
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
