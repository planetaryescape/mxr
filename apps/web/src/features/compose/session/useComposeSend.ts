import { assertWorkspaceBinding } from "@/lib/workspaceOwner";
import { prepareReviewedSend, retainSendReference } from "@/features/drafts/sendOutcomes";

import { useMutation, type QueryClient } from "@tanstack/react-query";
import type { useNavigate } from "@tanstack/react-router";
import { useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { toast } from "sonner";

import { UNDO_TOAST_DURATION_MS } from "@/components/ui/sonner";
import { VERB_FEEDBACK } from "@/features/mail-actions/verbFeedback";
import { archiveMessages } from "@/features/mailbox/api";
import { detectComposePromises } from "@/features/promises/api";
import { offerPromises } from "@/features/promises/promiseOffers";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { requestCoordinator } from "@/lib/requestCoordinator";
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
import { showSendCountdown } from "../SendCountdown";
import { acceptedDraftRevision } from "./acceptedDraftRevision";
import { forgetActiveDraft } from "./activeDrafts";
import {
  composeQueueKey,
  draftFingerprint,
  errorMessage,
  localComposeIssues,
  splitAddresses,
  type ComposeDraftState,
  type ComposeIntent,
} from "./composeDraft";
import { emitSendEvent } from "./sendEvents";

export interface ComposeSessionOptions {
  onSent?: () => void;
  onDiscarded?: () => void;
  onClose?: () => void;
}

interface ComposeSendInput {
  intent: ComposeIntent;
  options: ComposeSessionOptions;
  navigate: ReturnType<typeof useNavigate>;
  queryClient: QueryClient;
  draftRef: MutableRefObject<ComposeDraftState | null>;
  setDraft: Dispatch<SetStateAction<ComposeDraftState | null>>;
  saveCurrentDraft: () => Promise<ComposeSession | undefined>;
  isCurrentDraftSaved: (current: ComposeDraftState) => boolean;
  markSessionFinished: () => void;
  markSendAttempted: () => void;
  onValidationBlocked: () => void;
}

const KEPT_WHILE_DOWN = "Your message stays here. Send it once the daemon is back.";

export function useComposeSend({
  intent,
  options,
  navigate,
  queryClient,
  draftRef,
  setDraft,
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
  const archiveAfterSendRef = useRef(false);
  const remindAfterSendRef = useRef<{ at: Date; label: string } | null>(null);
  const [remindDialogOpen, setRemindDialogOpen] = useState(false);
  const sendLockRef = useRef(false);
  const [sendLocked, setSendLocked] = useState(false);
  const pendingCancelsRef = useRef(new Set<() => void>());
  const checkedFingerprintRef = useRef<string | null>(null);

  const sendSession = useMutation({
    mutationFn: ({
      draftPath,
      accountId,
      overrideToken,
      expectedRevision,
      operationId,
    }: {
      draftPath: string;
      accountId: string;
      overrideToken?: string;
      expectedRevision?: number | null;
      operationId: string;
    }) => sendComposeSession(draftPath, accountId, overrideToken, expectedRevision, operationId),
  });
  const scheduleSession = useMutation({
    mutationFn: async (at: Date) => {
      const current = draftRef.current;
      if (!current) throw new Error("No draft is open");
      let scheduledId: string | undefined;
      await requestCoordinator.queueComposeLatest(
        composeQueueKey(current.draftPath, current.recoverySourcePath),
        async () => {
          const active = draftRef.current;
          const revision =
            active?.draftPath === current.draftPath && active.draftId === current.draftId
              ? active.revision
              : current.revision;
          try {
            const response = await scheduleComposeSession({
              draftPath: current.draftPath,
              accountId: current.accountId,
              draftId: current.draftId,
              expectedRevision: revision,
              sendAt: at,
            });
            scheduledId = response.draft_id;
          } catch (error) {
            const accepted = acceptedDraftRevision(error);
            const latest = draftRef.current;
            if (
              accepted &&
              accepted.draft_id === current.draftId &&
              latest?.draftPath === current.draftPath &&
              latest.draftId === current.draftId
            ) {
              draftRef.current = { ...latest, revision: accepted.revision };
              setDraft((value) =>
                value?.draftPath === current.draftPath && value.draftId === current.draftId
                  ? { ...value, revision: accepted.revision }
                  : value,
              );
            }
            throw error;
          }
        },
      );
      if (!scheduledId) throw new Error("Scheduling returned no draft identity");
      return scheduledId;
    },
  });

  function requestSend() {
    startSendPipeline(false);
  }

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
    if (sendLockRef.current) return;
    if (refuseWhileDaemonDown("send", KEPT_WHILE_DOWN)) return;
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

  async function runSendPipeline() {
    await saveCurrentDraft().catch((error: Error) => {
      toast.error("Save before send failed", { description: error.message });
    });
    const current = draftRef.current;
    if (!current || !isCurrentDraftSaved(current)) {
      releaseSendLock();
      toast.error(
        current?.recoverySourcePath ? "Draft preserved locally" : "Draft changed while saving",
        {
          description: current?.recoverySourcePath
            ? "Use Retry or Open saved draft before sending."
            : "Retry send after the latest save.",
        },
      );
      return;
    }
    setCheckingSafety(true);
    setSafetyCheckError(null);
    checkedFingerprintRef.current = draftFingerprint(current);
    try {
      const { report } = await checkComposeSafety(
        current.draftPath,
        current.accountId,
        current.revision,
      );
      if (report.allowed && report.issues.length === 0) {
        setSafetyReport(null);
        dispatchSend();
        return;
      }
      setSafetyReport(report);
      setSendConfirmOpen(true);
    } catch (error) {
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
    if (refuseWhileDaemonDown("schedule a send", KEPT_WHILE_DOWN)) return;
    markSendAttempted();
    if (hasBlockingIssues(current)) {
      onValidationBlocked();
      return;
    }
    setSendLaterOpen(true);
  }

  async function scheduleSend(at: Date, label?: string) {
    if (refuseWhileDaemonDown("schedule a send", KEPT_WHILE_DOWN)) return;
    if (!acquireSendLock()) return;
    try {
      await runScheduleSend(at, label);
    } finally {
      releaseSendLock();
    }
  }

  async function runScheduleSend(at: Date, label?: string) {
    await saveCurrentDraft().catch((error: Error) => {
      toast.error("Save before schedule failed", { description: error.message });
    });
    const current = draftRef.current;
    if (!current || !isCurrentDraftSaved(current)) {
      toast.error(
        current?.recoverySourcePath ? "Draft preserved locally" : "Draft changed while saving",
        {
          description: current?.recoverySourcePath
            ? "Use Retry or Open saved draft before scheduling."
            : "Retry scheduling after the latest save.",
        },
      );
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
    forgetActiveDraft(intent.key, current?.binding);
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

  const setSendConfirmOpenFromUi: Dispatch<SetStateAction<boolean>> = (value) => {
    const next = typeof value === "function" ? value(sendConfirmOpen) : value;
    setSendConfirmOpen(next);
    if (!next) {
      setSafetyReport(null);
      setSafetyCheckError(null);
      releaseSendLock();
    }
  };

  async function confirmSend(override: boolean) {
    const blocked = Boolean(safetyReport && !safetyReport.allowed);
    const overrideToken =
      blocked && override
        ? (safetyReport?.issues.find((issue) => issue.override_token)?.override_token ?? undefined)
        : undefined;
    if (blocked && !overrideToken) return;
    if (refuseWhileDaemonDown("send", KEPT_WHILE_DOWN)) return;
    setSendConfirmOpen(false);
    setSafetyReport(null);
    setSafetyCheckError(null);
    const current = draftRef.current;
    if (current && draftFingerprint(current) !== checkedFingerprintRef.current) {
      void runSendPipeline();
      return;
    }
    dispatchSend(overrideToken);
  }

  function dispatchSend(overrideToken?: string) {
    const current = draftRef.current;
    if (!current || refuseWhileDaemonDown("send", KEPT_WHILE_DOWN)) {
      releaseSendLock();
      return;
    }
    const accountId = current.accountId;
    const draftPath = current.draftPath;
    const expectedRevision = current.revision;
    if (!current.draftId || expectedRevision == null) {
      releaseSendLock();
      toast.error("Save and review the draft before sending");
      return;
    }
    const sendReference = {
      binding: current.binding,
      operationId: `${current.draftId}:${expectedRevision}`,
      accountId,
      draftId: current.draftId,
    };
    const windowSeconds = useUiPrefs.getState().undoSendSeconds;
    const archiveSourceId =
      archiveAfterSendRef.current && intent.messageId ? intent.messageId : undefined;
    archiveAfterSendRef.current = false;
    const remind = remindAfterSendRef.current;
    remindAfterSendRef.current = null;
    const sendEvent = { intentKey: intent.key, sendId: `${intent.key}@${Date.now()}` };

    const afterSend = async (response: Awaited<ReturnType<typeof sendComposeSession>>) => {
      if (!sendReference.binding)
        throw new Error("Send owner is missing; retain this outcome for explicit recovery");
      assertWorkspaceBinding(sendReference.binding);
      if (response.state === "waiting") {
        toast.info("Waiting for the mail provider", {
          description: `Your approved message is retained and will retry automatically${response.retry_at ? ` at ${response.retry_at}` : ""}. Check Send outcomes in Drafts; do not resend.`,
        });
        void queryClient.invalidateQueries({ queryKey: ["send-outcome"] });
        return;
      }
      markSessionFinished();
      forgetActiveDraft(intent.key, sendReference.binding);
      emitSendEvent({
        kind: "sent",
        ...sendEvent,
        sentMessageId: response.message_id ?? undefined,
      });
      toast.success(VERB_FEEDBACK.send.pastTense);
      if (remind)
        await setReminderAfterSend(response.message_id ?? undefined, remind, sendReference.binding);
      if (archiveSourceId) {
        try {
          assertWorkspaceBinding(sendReference.binding);
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
    };
    const fire = async () => {
      let reviewedReference;
      try {
        if (sendReference.binding) assertWorkspaceBinding(sendReference.binding);
        reviewedReference = await prepareReviewedSend(sendReference);
        retainSendReference(reviewedReference);
      } catch (error) {
        setPendingSends((count) => Math.max(0, count - 1));
        releaseSendLock();
        toast.error("Cannot retain send identity", { description: errorMessage(error) });
        return;
      }
      sendSession
        .mutateAsync({
          draftPath,
          accountId,
          overrideToken,
          expectedRevision,
          operationId: reviewedReference.operationId,
        })
        .then(
          (response) =>
            afterSend(response).catch((error: unknown) =>
              toast.error("Sent, but something after the send failed", {
                description: errorMessage(error),
              }),
            ),
          (err: Error) => {
            emitSendEvent({ kind: "failed", ...sendEvent });
            toast.error("Send outcome needs checking", {
              description: `${err.message} Operation ${reviewedReference.operationId}; check Send outcomes in Drafts before trying again.`,
            });
          },
        )
        .finally(() => {
          setPendingSends((count) => Math.max(0, count - 1));
          releaseSendLock();
        });
    };

    setPendingSends((count) => count + 1);
    emitSendEvent({ kind: "queued", ...sendEvent });
    void detectComposePromises(draftPath, accountId)
      .then((detection) => offerPromises(sendEvent.sendId, detection))
      .catch(() => undefined);
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
      emitSendEvent({ kind: "cancelled", ...sendEvent });
      toast.info("Send cancelled");
    };
    const toastId = showSendCountdown({
      seconds: windowSeconds,
      summary: sendSummary(current),
      onUndo: cancel,
    });
    const timer = window.setTimeout(() => {
      if (cancelled) return;
      pendingCancelsRef.current.delete(cancelThis);
      useUndo.getState().clearPendingSendCancel(cancel);
      toast.dismiss(toastId);
      if (refuseWhileDaemonDown("send", KEPT_WHILE_DOWN)) {
        setPendingSends((count) => Math.max(0, count - 1));
        releaseSendLock();
        emitSendEvent({ kind: "cancelled", ...sendEvent });
        return;
      }
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
    scheduling: scheduleSession.isPending || sendLocked,
    confirmSend,
    sendPending: sendSession.isPending,
  };
}

function hasBlockingIssues(draft: ComposeDraftState): boolean {
  return localComposeIssues(draft).some((issue) => issue.severity === "error");
}

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
  binding: import("@/lib/workspaceOwner").WorkspaceBinding,
) {
  if (!sentMessageId) {
    toast.warning("Sent, but no reminder was set", {
      description: "The mxr bridge did not return the sent message id.",
    });
    return;
  }
  try {
    await setAutoReminder(sentMessageId, remind.at, binding);
  } catch (error) {
    toast.error("Sent, but the reminder failed", { description: errorMessage(error) });
    return;
  }
  toast.success(`Reminder set for ${remind.label} if no reply`, {
    duration: UNDO_TOAST_DURATION_MS,
    action: {
      label: "Cancel",
      onClick: () => {
        cancelAutoReminder(sentMessageId, binding)
          .then(() => toast.success("Reminder cancelled"))
          .catch((error: unknown) =>
            toast.error("Couldn't cancel the reminder", { description: errorMessage(error) }),
          );
      },
    },
  });
}
