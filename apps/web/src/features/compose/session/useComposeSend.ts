/*
 * Send pipeline for a compose session: validate → save → safety check →
 * (confirm dialog when the report has issues) → deferred dispatch with an
 * undo window. Also owns send-later scheduling, which shares the same
 * validate-and-save front half.
 */

import { useMutation, type QueryClient } from "@tanstack/react-query";
import type { useNavigate } from "@tanstack/react-router";
import { useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { toast } from "sonner";

import { archiveMessages } from "@/features/mailbox/api";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { useUndo } from "@/state/undoStore";
import {
  cancelAutoReminder,
  cancelScheduledSend,
  checkComposeSafety,
  scheduleComposeSession,
  sendComposeSession,
  setAutoReminder,
  type ComposeSession,
  type DraftSafetyReport,
} from "../api";
import { forgetActiveDraft } from "./activeDrafts";
import {
  draftFingerprint,
  errorMessage,
  localComposeIssues,
  splitAddresses,
  type ComposeDraftState,
  type ComposeIntent,
} from "./composeDraft";

export interface ComposeSessionOptions {
  /** Called after a successful send instead of the default
   * navigate-to-Sent (surface hosts close in place). */
  onSent?: () => void;
  /** Called after a successful discard instead of navigating to inbox. */
  onDiscarded?: () => void;
  /** Called by `requestClose` once pending edits are saved. */
  onClose?: () => void;
}

interface ComposeSendInput {
  intent: ComposeIntent;
  options: ComposeSessionOptions;
  navigate: ReturnType<typeof useNavigate>;
  queryClient: QueryClient;
  draftRef: MutableRefObject<ComposeDraftState | null>;
  saveCurrentDraft: () => Promise<ComposeSession | undefined>;
  isCurrentDraftSaved: (current: ComposeDraftState) => boolean;
  markSessionFinished: () => void;
  /** Called on every Send / Send later press, before validation. */
  markSendAttempted: () => void;
  /** Called when local validation blocks the attempt; the issues are shown
   * inline, so this only moves focus to help fix them. */
  onValidationBlocked: () => void;
}

export function useComposeSend({
  intent,
  options,
  navigate,
  queryClient,
  draftRef,
  saveCurrentDraft,
  isCurrentDraftSaved,
  markSessionFinished,
  markSendAttempted,
  onValidationBlocked,
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
  // Same lifecycle for "send and remind me": set per run, consumed at
  // dispatch, dropped by a cancelled undo window.
  const remindAfterSendRef = useRef<{ at: Date; label: string } | null>(null);
  const [remindDialogOpen, setRemindDialogOpen] = useState(false);
  // One send per session at a time. Held from the first Send press through
  // save, safety check, the confirm dialog and the undo window until the
  // send settles or is cancelled. A ref, not state: two presses in the same
  // tick must both see it.
  const sendLockRef = useRef(false);
  const [sendLocked, setSendLocked] = useState(false);
  // Every undo-window cancel still open for this session. The lock means
  // there should only ever be one, but cancel clears them all regardless.
  const pendingCancelsRef = useRef(new Set<() => void>());
  // Fingerprint of the draft the current safety report was computed for;
  // an edit made from the confirm dialog (adding a suggested Cc) means the
  // report is stale and has to be re-run before anything is sent.
  const checkedFingerprintRef = useRef<string | null>(null);

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
      // Editing a stored draft reschedules it in place; a new compose
      // session lets the bridge mint the stored draft's id.
      const response = await scheduleComposeSession({
        draftPath: current.draftPath,
        accountId: current.accountId,
        draftId: intent.draftId,
        sendAt: at,
      });
      return response.draft_id;
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

  function acquireSendLock(): boolean {
    if (sendLockRef.current) return false;
    sendLockRef.current = true;
    setSendLocked(true);
    return true;
  }

  function releaseSendLock() {
    sendLockRef.current = false;
    setSendLocked(false);
  }

  /** Send, then ask the daemon to remind the user if nobody replies by
   * `at` (TUI parity: `n` in the send-confirm modal). */
  function requestSendAndRemind(at: Date, label: string) {
    setRemindDialogOpen(false);
    startSendPipeline(false, { at, label });
  }

  function startSendPipeline(
    archiveAfterSend: boolean,
    remind: { at: Date; label: string } | null = null,
  ) {
    const current = draftRef.current;
    if (!current) return;
    // Refuse before validation so a second press never re-runs anything.
    if (sendLockRef.current) return;
    markSendAttempted();
    if (hasBlockingIssues(current)) {
      onValidationBlocked();
      return;
    }
    if (!acquireSendLock()) return;
    archiveAfterSendRef.current = archiveAfterSend;
    remindAfterSendRef.current = remind;
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
      releaseSendLock();
      toast.error("Draft changed while saving", {
        description: "Retry send after the latest save.",
      });
      return;
    }
    setCheckingSafety(true);
    setSafetyCheckError(null);
    checkedFingerprintRef.current = draftFingerprint(current);
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
    if (sendLockRef.current) return;
    markSendAttempted();
    if (hasBlockingIssues(current)) {
      onValidationBlocked();
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
    let scheduledDraftId: string;
    try {
      scheduledDraftId = await scheduleSession.mutateAsync(at);
    } catch (error) {
      toast.error("Schedule failed", { description: errorMessage(error) });
      return;
    }
    setSendLaterOpen(false);
    markSessionFinished();
    forgetActiveDraft(intent.key);
    // The exact time, so the user sees what was stored whichever way it
    // was picked.
    toast.success(label ? `Send scheduled for ${label}` : "Send scheduled", {
      duration: 10_000,
      action: {
        label: "Cancel",
        onClick: () => {
          cancelScheduledSend(scheduledDraftId)
            .then(() => {
              toast.success("Scheduled send cancelled", {
                description: "The message is kept in Drafts.",
              });
              void queryClient.invalidateQueries({ queryKey: ["drafts"] });
            })
            .catch((error: unknown) =>
              toast.error("Couldn't cancel the scheduled send", {
                description: errorMessage(error),
              }),
            );
        },
      },
    });
    void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    if (options.onSent) {
      options.onSent();
    } else {
      await navigate({ to: "/m/$mailbox", params: { mailbox: "sent" } });
    }
  }

  /** Dialog-facing open setter: closing without confirming abandons this
   * send attempt, so the lock is released for the next one. */
  const setSendConfirmOpenFromUi: Dispatch<SetStateAction<boolean>> = (value) => {
    const next = typeof value === "function" ? value(sendConfirmOpen) : value;
    setSendConfirmOpen(next);
    if (!next) {
      setSafetyReport(null);
      setSafetyCheckError(null);
      releaseSendLock();
    }
  };

  /** Confirm from the safety dialog. A blocked report only sends when the
   * user explicitly overrode it, using the report's override token. */
  async function confirmSend(override: boolean) {
    const blocked = Boolean(safetyReport && !safetyReport.allowed);
    const overrideToken =
      blocked && override
        ? (safetyReport?.issues.find((issue) => issue.override_token)?.override_token ?? undefined)
        : undefined;
    if (blocked && !overrideToken) return;
    setSendConfirmOpen(false);
    setSafetyReport(null);
    setSafetyCheckError(null);
    const current = draftRef.current;
    if (current && draftFingerprint(current) !== checkedFingerprintRef.current) {
      // Still holding the send lock: re-save and re-check the edited draft.
      void runSendPipeline();
      return;
    }
    dispatchSend(overrideToken);
  }

  /** Deferred dispatch with a configurable undo window. The pending window
   * counts as unsaved work so beforeunload warns — closing the tab here
   * would silently drop the send. Cancellable via the toast or global z. */
  function dispatchSend(overrideToken?: string) {
    const current = draftRef.current;
    if (!current) {
      releaseSendLock();
      return;
    }
    const accountId = current.accountId;
    const draftPath = current.draftPath;
    const windowSeconds = useUiPrefs.getState().undoSendSeconds;
    const archiveSourceId =
      archiveAfterSendRef.current && intent.messageId ? intent.messageId : undefined;
    archiveAfterSendRef.current = false;
    const remind = remindAfterSendRef.current;
    remindAfterSendRef.current = null;

    const fire = () => {
      sendSession
        .mutateAsync({ draftPath, accountId, overrideToken })
        .then(async (response) => {
          markSessionFinished();
          forgetActiveDraft(intent.key);
          toast.success("Message sent");
          if (remind) await setReminderAfterSend(response.message_id ?? undefined, remind);
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
        .finally(() => {
          setPendingSends((count) => Math.max(0, count - 1));
          releaseSendLock();
        });
    };

    setPendingSends((count) => count + 1);
    if (windowSeconds === 0) {
      fire();
      return;
    }

    let cancelled = false;
    const cancelThis = () => {
      if (cancelled) return;
      cancelled = true;
      window.clearTimeout(timer);
      pendingCancelsRef.current.delete(cancelThis);
      setPendingSends((count) => Math.max(0, count - 1));
      toast.dismiss(toastId);
    };
    const cancel = () => {
      for (const pending of pendingCancelsRef.current) pending();
      useUndo.getState().clearPendingSendCancel(cancel);
      releaseSendLock();
      toast.info("Send cancelled");
    };
    const toastId = toast(`Sending in ${windowSeconds}s`, {
      duration: windowSeconds * 1000,
      description: sendSummary(current),
      action: { label: "Undo", onClick: cancel },
    });
    const timer = window.setTimeout(() => {
      if (cancelled) return;
      pendingCancelsRef.current.delete(cancelThis);
      useUndo.getState().clearPendingSendCancel(cancel);
      fire();
    }, windowSeconds * 1000);
    pendingCancelsRef.current.add(cancelThis);
    useUndo.getState().setPendingSendCancel(cancel);
  }

  return {
    remindDialogOpen,
    setRemindDialogOpen,
    requestSendAndRemind,
    sendConfirmOpen,
    setSendConfirmOpen: setSendConfirmOpenFromUi,
    /** True from the first Send press until that send settles or is
     * cancelled, including the whole undo window. */
    sendLocked,
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

function hasBlockingIssues(draft: ComposeDraftState): boolean {
  return localComposeIssues(draft).some((issue) => issue.severity === "error");
}

/** "To ada@x.com and 2 more, from me@y.com. Press z to cancel." */
function sendSummary(draft: ComposeDraftState): string {
  const recipients = splitAddresses(
    `${draft.frontmatter.to},${draft.frontmatter.cc},${draft.frontmatter.bcc}`,
  );
  const more = recipients.length > 1 ? ` and ${recipients.length - 1} more` : "";
  const from = draft.frontmatter.from.trim();
  return `To ${recipients[0] ?? "no one"}${more}${from ? `, from ${from}` : ""}. Press z to cancel.`;
}

async function setReminderAfterSend(
  sentMessageId: string | undefined,
  remind: { at: Date; label: string },
) {
  if (!sentMessageId) {
    toast.warning("Sent, but no reminder was set", {
      description: "The mxr bridge did not return the sent message id.",
    });
    return;
  }
  try {
    await setAutoReminder(sentMessageId, remind.at);
  } catch (error) {
    toast.error("Sent, but the reminder failed", { description: errorMessage(error) });
    return;
  }
  toast.success(`Reminder set for ${remind.label}`, {
    description: "If nobody replies by then.",
    duration: 10_000,
    action: {
      label: "Cancel",
      onClick: () => {
        cancelAutoReminder(sentMessageId)
          .then(() => toast.success("Reminder cancelled"))
          .catch((error: unknown) =>
            toast.error("Couldn't cancel the reminder", { description: errorMessage(error) }),
          );
      },
    },
  });
}
