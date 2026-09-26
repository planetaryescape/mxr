/*
 * Send pipeline for a compose session: validate → save → safety check →
 * (confirm dialog when the report has issues) → deferred dispatch with an
 * undo window. Also owns send-later scheduling, which shares the same
 * validate-and-save front half.
 */

import { useMutation, type QueryClient } from "@tanstack/react-query";
import type { useNavigate } from "@tanstack/react-router";
import { useRef, useState, type MutableRefObject } from "react";
import { toast } from "sonner";

import { archiveMessages } from "@/features/mailbox/api";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { useUndo } from "@/state/undoStore";
import {
  checkComposeSafety,
  createScheduledSend,
  saveLocalDraft,
  sendComposeSession,
  type ComposeSession,
  type DraftSafetyReport,
} from "../api";
import { forgetActiveDraft } from "./activeDrafts";
import {
  draftIntentFromKind,
  errorMessage,
  localComposeIssues,
  parseDraftAddresses,
  type ComposeDraftState,
  type ComposeIntent,
} from "./composeDraft";

export interface ComposeSessionOptions {
  /** Called after a successful send instead of the default
   * navigate-to-Sent (surface hosts close in place). */
  onSent?: () => void;
  /** Called after a successful discard instead of navigating to inbox. */
  onDiscarded?: () => void;
}

interface ComposeSendInput {
  intent: ComposeIntent;
  options: ComposeSessionOptions;
  navigate: ReturnType<typeof useNavigate>;
  queryClient: QueryClient;
  draftRef: MutableRefObject<ComposeDraftState | null>;
  saveCurrentDraft: () => Promise<ComposeSession | undefined>;
  isCurrentDraftSaved: (current: ComposeDraftState) => boolean;
}

export function useComposeSend({
  intent,
  options,
  navigate,
  queryClient,
  draftRef,
  saveCurrentDraft,
  isCurrentDraftSaved,
}: ComposeSendInput) {
  const [sendConfirmOpen, setSendConfirmOpen] = useState(false);
  const [sendLaterOpen, setSendLaterOpen] = useState(false);
  const [pendingSends, setPendingSends] = useState(0);
  const [safetyReport, setSafetyReport] = useState<DraftSafetyReport | null>(null);
  const [safetyCheckError, setSafetyCheckError] = useState<string | null>(null);
  const [checkingSafety, setCheckingSafety] = useState(false);
  // Set per send pipeline run (cmd+shift+Enter); consumed at dispatch time so
  // the safety-dialog detour keeps the archive intent and a cancelled undo
  // window drops it.
  const archiveAfterSendRef = useRef(false);

  const sendSession = useMutation({
    mutationFn: ({
      draftPath,
      accountId,
      overrideToken,
    }: {
      draftPath: string;
      accountId: string;
      overrideToken?: string;
    }) => sendComposeSession(draftPath, accountId, overrideToken),
  });
  const scheduleSession = useMutation({
    mutationFn: async (at: Date) => {
      const current = draftRef.current;
      if (!current) throw new Error("No draft is open");
      const now = new Date().toISOString();
      // Editing an existing stored draft must save it in place; only mint a
      // fresh id for a genuinely new compose session (save-local is an
      // upsert-by-id, so reusing the id updates rather than duplicates it).
      const draftId = intent.draftId ?? crypto.randomUUID();
      await saveLocalDraft({
        id: draftId,
        account_id: current.accountId,
        intent: draftIntentFromKind(current.kind),
        to: parseDraftAddresses(current.frontmatter.to),
        cc: parseDraftAddresses(current.frontmatter.cc),
        bcc: parseDraftAddresses(current.frontmatter.bcc),
        subject: current.frontmatter.subject,
        body_markdown: current.bodyMarkdown,
        attachments: [...current.frontmatter.attach],
        created_at: now,
        updated_at: now,
      });
      await createScheduledSend(draftId, at);
    },
  });

  function requestSend() {
    startSendPipeline(false);
  }

  /** cmd+shift+Enter: send, then archive the source conversation (replies
   * only — a new message has no source to archive). */
  function requestSendAndArchive() {
    startSendPipeline(true);
  }

  function startSendPipeline(archiveAfterSend: boolean) {
    const current = draftRef.current;
    if (!current) return;
    const errors = localComposeIssues(current).filter((issue) => issue.severity === "error");
    if (errors.length > 0) {
      toast.error("Fix compose errors before sending", { description: errors[0]?.message });
      return;
    }
    archiveAfterSendRef.current = archiveAfterSend;
    void runSendPipeline();
  }

  /** Save → safety check → clean drafts dispatch straight away; reports
   * with issues (or a failed check) open the confirm dialog instead. */
  async function runSendPipeline() {
    await saveCurrentDraft().catch((error: Error) => {
      toast.error("Save before send failed", { description: error.message });
    });
    const current = draftRef.current;
    if (!current || !isCurrentDraftSaved(current)) {
      toast.error("Draft changed while saving", {
        description: "Retry send after the latest save.",
      });
      return;
    }
    setCheckingSafety(true);
    setSafetyCheckError(null);
    try {
      const { report } = await checkComposeSafety(current.draftPath, current.accountId);
      if (report.allowed && report.issues.length === 0) {
        setSafetyReport(null);
        dispatchSend();
        return;
      }
      setSafetyReport(report);
      setSendConfirmOpen(true);
    } catch (error) {
      // Fail closed into the dialog, not into a silent send.
      setSafetyReport(null);
      setSafetyCheckError(errorMessage(error));
      setSendConfirmOpen(true);
    } finally {
      setCheckingSafety(false);
    }
  }

  function requestSendLater() {
    const current = draftRef.current;
    if (!current) return;
    const errors = localComposeIssues(current).filter((issue) => issue.severity === "error");
    if (errors.length > 0) {
      toast.error("Fix compose errors before scheduling", { description: errors[0]?.message });
      return;
    }
    setSendLaterOpen(true);
  }

  /** Save → store as a local draft → schedule. Closes the composer like a
   * send; the daemon dispatches the stored draft at `at`. */
  async function scheduleSend(at: Date, label?: string) {
    await saveCurrentDraft().catch((error: Error) => {
      toast.error("Save before schedule failed", { description: error.message });
    });
    const current = draftRef.current;
    if (!current || !isCurrentDraftSaved(current)) {
      toast.error("Draft changed while saving", {
        description: "Retry scheduling after the latest save.",
      });
      return;
    }
    try {
      await scheduleSession.mutateAsync(at);
    } catch (error) {
      toast.error("Schedule failed", { description: errorMessage(error) });
      return;
    }
    setSendLaterOpen(false);
    forgetActiveDraft(intent.key);
    toast.success("Send scheduled", {
      description: label ? `Sends ${label}` : undefined,
    });
    void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    if (options.onSent) {
      options.onSent();
    } else {
      await navigate({ to: "/m/$mailbox", params: { mailbox: "sent" } });
    }
  }

  /** Confirm from the safety dialog. Picks up the override token from the
   * report's blocking issue when one exists. */
  async function confirmSend() {
    const overrideToken =
      safetyReport && !safetyReport.allowed
        ? (safetyReport.issues.find((issue) => issue.override_token)?.override_token ?? undefined)
        : undefined;
    setSendConfirmOpen(false);
    setSafetyReport(null);
    setSafetyCheckError(null);
    dispatchSend(overrideToken);
  }

  /** Deferred dispatch with a configurable undo window. The pending window
   * counts as unsaved work so beforeunload warns — closing the tab here
   * would silently drop the send. Cancellable via the toast or global z. */
  function dispatchSend(overrideToken?: string) {
    const current = draftRef.current;
    if (!current) return;
    const accountId = current.accountId;
    const draftPath = current.draftPath;
    const windowSeconds = useUiPrefs.getState().undoSendSeconds;
    const archiveSourceId =
      archiveAfterSendRef.current && intent.messageId ? intent.messageId : undefined;
    archiveAfterSendRef.current = false;

    const fire = () => {
      sendSession
        .mutateAsync({ draftPath, accountId, overrideToken })
        .then(async () => {
          forgetActiveDraft(intent.key);
          toast.success("Message sent");
          if (archiveSourceId) {
            try {
              await archiveMessages([archiveSourceId]);
              void queryClient.invalidateQueries({ queryKey: ["mailbox"] });
              void queryClient.invalidateQueries({ queryKey: ["thread"] });
              toast.success("Conversation archived");
            } catch (error) {
              toast.error("Archive after send failed", { description: errorMessage(error) });
            }
          }
          if (options.onSent) {
            options.onSent();
          } else {
            await navigate({ to: "/m/$mailbox", params: { mailbox: "sent" } });
          }
        })
        .catch((err: Error) => toast.error("Send failed", { description: err.message }))
        .finally(() => setPendingSends((count) => Math.max(0, count - 1)));
    };

    setPendingSends((count) => count + 1);
    if (windowSeconds === 0) {
      fire();
      return;
    }

    let cancelled = false;
    const cancel = () => {
      if (cancelled) return;
      cancelled = true;
      window.clearTimeout(timer);
      useUndo.getState().setPendingSendCancel(null);
      setPendingSends((count) => Math.max(0, count - 1));
      toast.dismiss(toastId);
      toast.info("Send cancelled");
    };
    const toastId = toast(`Sending in ${windowSeconds}s`, {
      duration: windowSeconds * 1000,
      description: "z to cancel",
      action: { label: "Undo", onClick: cancel },
    });
    const timer = window.setTimeout(() => {
      if (cancelled) return;
      useUndo.getState().setPendingSendCancel(null);
      fire();
    }, windowSeconds * 1000);
    useUndo.getState().setPendingSendCancel(cancel);
  }

  return {
    sendConfirmOpen,
    setSendConfirmOpen,
    sendLaterOpen,
    setSendLaterOpen,
    pendingSends,
    safetyReport,
    safetyCheckError,
    checkingSafety,
    requestSend,
    requestSendAndArchive,
    requestSendLater,
    scheduleSend,
    scheduling: scheduleSession.isPending,
    confirmSend,
    sendPending: sendSession.isPending,
  };
}
