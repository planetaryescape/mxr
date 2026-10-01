/*
 * Mail verbs, written once. The list and the reader each hand in a way to
 * get their current target plus what to do after a message leaves the
 * view; everything else (mutations, dialogs, compose, rails) is shared.
 */

import { toast } from "sonner";

import { cancelAutoReminder } from "@/features/compose/api";
import { newMessageIntent, replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import {
  cancelInviteResponse,
  respondToInvite,
  type InviteAction,
} from "@/features/invites/inviteResponse";
import { openInviteComment } from "@/features/invites/useInviteResponse";
import { fetchSenderProfile, getThreadBriefing } from "@/features/mailbox/api";
import { deskLaneOf } from "@/features/desk/deskLater";
import { apiFetch } from "@/api/client";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import type { ScopeController } from "@/lib/keys/controllers";
import { parseAddress, plural } from "@/lib/format";
import { useModals } from "@/state/modalStore";

import { openMailDialog } from "./mailDialogStore";
import { performMailAction, refuseMailActionWhileDaemonDown } from "./mailMutations";
import type { MailAction } from "./pendingMailOps";
import { ensureThread, type MailTarget } from "./target";

export interface MailVerbHooks {
  getTarget: () => MailTarget | null;
  /** Runs right after a verb that takes the target out of the view. */
  afterLeave?: (action: MailAction) => void;
  /** Where replies open: inline under the thread, or an overlay. */
  composeSurface: "inline" | "overlay";
  /**
   * Let a view handle part of a mutation its own way (the desk: archive on
   * a desk row is Done). Returns what is left for the normal path (null
   * when the view takes all of it) and the view's own change, which runs
   * with the rest, after any confirmation.
   */
  intercept?: (action: MailAction, target: MailTarget) => InterceptedVerb;
}

export interface InterceptedVerb {
  rest: MailTarget | null;
  commit?: () => void;
}

/** Bulk trash/spam and very large batches confirm first; the rest undo. */
const CONFIRM_THRESHOLD = 20;

function needsConfirm(action: MailAction, target: MailTarget): boolean {
  const count = target.messageIds.length;
  if (count > CONFIRM_THRESHOLD) return true;
  return (action === "trash" || action === "spam") && target.conversations > 1;
}

function single(target: MailTarget | null, what: string): MailTarget | null {
  if (!target) return null;
  if (!target.threadId || !target.primary) {
    toast.info(`${what} works on one conversation at a time`);
    return null;
  }
  return target;
}

/**
 * Star or unstar as the target shows. One conversation toggles; a
 * multi-selection always stars, as in the TUI. No preview, even when the
 * unstar reaches starred messages outside the list: it is one conversation,
 * not a batch, and `u` undoes it (the CLI's `--dry-run` previews batches).
 */
export function toggleStar(target: MailTarget): void {
  if (target.conversations <= 1 && target.anyStarred) {
    void performMailAction("unstar", target.unstarIds);
  } else {
    void performMailAction("star", target.messageIds);
  }
}

export function createMailVerbs(hooks: MailVerbHooks): ScopeController {
  const mutate = (action: MailAction) => () => {
    const requested = hooks.getTarget();
    if (!requested || requested.messageIds.length === 0) return;
    // Before the view's own part (the desk's Done) and before it moves on.
    if (refuseMailActionWhileDaemonDown(action)) return;
    const { rest: target, commit } = hooks.intercept?.(action, requested) ?? { rest: requested };
    if (!target || target.messageIds.length === 0) {
      commit?.();
      hooks.afterLeave?.(action);
      return;
    }
    const run = () => {
      commit?.();
      void performMailAction(action, target.messageIds);
      if (action !== "star" && action !== "unstar" && action !== "read" && action !== "unread") {
        hooks.afterLeave?.(action);
      }
    };
    if (needsConfirm(action, target)) {
      openMailDialog({ kind: "confirm", target, action, onConfirm: run });
    } else {
      run();
    }
  };

  const openReply = (mode: "single" | "all" | "forward") => () => {
    const target = single(hooks.getTarget(), mode === "forward" ? "Forward" : "Reply");
    if (!target?.primary) return;
    useComposeUi.getState().openCompose(replyIntent(target.primary.id, mode), hooks.composeSurface);
  };

  const rsvp = (action: InviteAction, withComment: boolean) => () => {
    const target = single(hooks.getTarget(), "RSVP");
    if (!target?.threadId || refuseWhileDaemonDown("answer the invite")) return;
    void ensureThread(target.threadId).then((thread) => {
      const invite = thread.bodies.findLast((body) => body.metadata?.calendar);
      if (!invite) {
        toast.info("This conversation has no calendar invite");
        return;
      }
      if (withComment) {
        openInviteComment(invite.message_id, action, hooks.composeSurface);
      } else {
        respondToInvite(invite.message_id, action);
      }
    });
  };

  return {
    archive: mutate("archive"),
    readArchive: mutate("read-and-archive"),
    trash: mutate("trash"),
    spam: mutate("spam"),
    markRead: mutate("read"),
    markUnread: mutate("unread"),
    toggleStar: () => {
      const target = hooks.getTarget();
      if (target) toggleStar(target);
    },
    label: () => {
      const target = hooks.getTarget();
      if (target && !refuseMailActionWhileDaemonDown("labels"))
        openMailDialog({ kind: "labels", target });
    },
    move: () => {
      const target = hooks.getTarget();
      if (target && !refuseMailActionWhileDaemonDown("move")) {
        openMailDialog({ kind: "move", target, onDone: () => hooks.afterLeave?.("move") });
      }
    },
    snooze: () => {
      const target = hooks.getTarget();
      if (target && !refuseMailActionWhileDaemonDown("snooze")) {
        openMailDialog({ kind: "snooze", target, onDone: () => hooks.afterLeave?.("snooze") });
      }
    },
    unsubscribe: () => {
      const target = single(hooks.getTarget(), "Unsubscribe");
      if (target && !refuseWhileDaemonDown("unsubscribe")) {
        openMailDialog({
          kind: "unsubscribe",
          target,
          onDone: () => hooks.afterLeave?.("archive"),
        });
      }
    },
    replyLater: () => {
      const target = single(hooks.getTarget(), "Reply later");
      if (!target?.primary || !target.threadId || refuseWhileDaemonDown("set reply later")) {
        return;
      }
      openMailDialog({
        kind: "reply-later",
        target,
        waiting: deskLaneOf(target.threadId) === "waiting",
      });
    },
    reply: openReply("single"),
    replyAll: openReply("all"),
    forward: openReply("forward"),
    composeToSender: () => {
      const target = single(hooks.getTarget(), "Write to sender");
      const email = parseAddress(target?.primary?.sender_detail ?? target?.primary?.sender).email;
      if (!email) return;
      useComposeUi
        .getState()
        .openCompose(
          { ...newMessageIntent(), key: `compose:new:${email}`, prefillTo: email },
          "overlay",
        );
    },
    exportThread: () => {
      const target = single(hooks.getTarget(), "Export");
      if (!target?.threadId) return;
      apiFetch<{ content: string }>(
        `/api/v1/mail/threads/${encodeURIComponent(target.threadId)}/export`,
      )
        .then(({ content }) => {
          downloadText(`${slug(target.primary?.subject ?? "thread")}.md`, content, "text/markdown");
          toast.success("Exported thread as Markdown");
        })
        .catch((error: Error) => toast.error("Export failed", { description: error.message }));
    },
    briefing: () => {
      const target = single(hooks.getTarget(), "Briefing");
      if (!target?.threadId) return;
      const toastId = toast.loading("Preparing briefing…");
      getThreadBriefing({ threadId: target.threadId, refresh: false })
        .then((result) => {
          toast.dismiss(toastId);
          useModals.getState().openRightRail("thread-briefing", result.briefing);
        })
        .catch((error: Error) =>
          toast.error("Briefing failed", { id: toastId, description: error.message }),
        );
    },
    senderProfile: () => {
      const target = single(hooks.getTarget(), "Sender profile");
      const email = parseAddress(target?.primary?.sender_detail ?? target?.primary?.sender).email;
      if (!target?.accountId || !email) return;
      fetchSenderProfile({ accountId: target.accountId, email })
        .then((result) => useModals.getState().openRightRail("sender-profile", result))
        .catch((error: Error) =>
          toast.error("Sender profile failed", { description: error.message }),
        );
    },
    whois: () => {
      const target = single(hooks.getTarget(), "Whois");
      const email = parseAddress(target?.primary?.sender_detail ?? target?.primary?.sender).email;
      if (!email) return;
      useModals.getState().openRightRail("whois", { entity: email, accountId: target?.accountId });
    },
    links: () => {
      const target = single(hooks.getTarget(), "Links");
      if (target) openMailDialog({ kind: "links", target });
    },
    attachments: () => {
      const target = single(hooks.getTarget(), "Attachments");
      if (!target?.threadId) return;
      void ensureThread(target.threadId).then((thread) => {
        const attachments = thread.bodies.flatMap((body) => body.attachments ?? []);
        if (attachments.length === 0) {
          toast.info("No attachments in this conversation");
          return;
        }
        useModals.getState().openRightRail("attachments", attachments);
      });
    },
    cancelReminder: () => {
      const target = single(hooks.getTarget(), "Cancel reminder");
      if (!target?.primary || refuseWhileDaemonDown("cancel the reminder")) return;
      cancelAutoReminder(target.primary.id)
        .then(() => toast.success("Reminder cancelled"))
        .catch((error: Error) =>
          toast.error("Couldn't cancel the reminder", { description: error.message }),
        );
    },
    inviteAccept: rsvp("accept", false),
    inviteTentative: rsvp("tentative", false),
    inviteDecline: rsvp("decline", false),
    inviteAcceptComment: rsvp("accept", true),
    inviteTentativeComment: rsvp("tentative", true),
    inviteDeclineComment: rsvp("decline", true),
    inviteCancel: () => {
      const target = hooks.getTarget();
      const cancelled = target?.messageIds.some((id) => cancelInviteResponse(id));
      if (!cancelled) toast.info("No RSVP waiting to send");
    },
  };
}

export function describeTarget(target: MailTarget): string {
  if (target.conversations === 1) return target.primary?.subject || "(no subject)";
  return plural(target.conversations, "conversation");
}

function downloadText(filename: string, content: string, type: string) {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function slug(value: string): string {
  return (
    value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "")
      .slice(0, 60) || "thread"
  );
}
