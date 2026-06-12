/*
 * Mailbox-side palette actions: label-add, label-remove, move, read-and-archive,
 * unsubscribe. Visible when the user has either a focused thread or a non-empty
 * selection. Runners read selection from the store at invocation time.
 *
 * The actual API mutations route through `useOptimisticMailMutation` inside
 * the picker components — these registry runners just open the right panel.
 */

import { Archive, ArrowRightCircle, Route as RouteIcon, Tag, MailX, Undo2 } from "lucide-react";
import { toast } from "sonner";

import {
  readAndArchiveMessages,
  unsubscribeAndClearSender,
  unsubscribeFromSender,
} from "@/features/mailbox/api";
import { performUndo } from "@/features/mailbox/useOptimisticMailMutation";
import { useUndo } from "@/state/undoStore";
import type { ThreadResponse } from "@/features/mailbox/types";
import type { Action } from "@/lib/actions/types";
import { getActiveQueryClient } from "@/lib/queryClient";
import { or, withFocusedThread, withSelection } from "@/lib/actions/when";
import { useModals } from "@/state/modalStore";
import { useSelection } from "@/state/selectionStore";

function focusedThreadId(): string | null {
  if (typeof window === "undefined") return null;
  const match = window.location.pathname.match(/^\/m\/[^/]+\/([^/]+)/);
  return match?.[1] ?? null;
}

function cachedThread(threadId: string): ThreadResponse | undefined {
  return getActiveQueryClient()?.getQueryData<ThreadResponse>(["thread", threadId]);
}

function cachedThreadMessageIds(threadId: string): string[] {
  return cachedThread(threadId)?.messages.map((message) => message.id) ?? [];
}

function focusedSender(): { address: string; accountId?: string } | null {
  const threadId = focusedThreadId();
  if (!threadId) return null;
  const message = cachedThread(threadId)?.messages[0];
  const address = message?.sender_detail ?? message?.sender;
  if (!address || !address.includes("@")) return null;
  return { address, accountId: message?.account_id };
}

function activeRouteQueueLabel(): string | null {
  if (typeof window === "undefined") return null;
  const match = window.location.pathname.match(/^\/m\/label\/([^/]+)/);
  return match?.[1] ? decodeURIComponent(match[1]) : null;
}

function targetMessageIds(): string[] {
  const ids = Array.from(useSelection.getState().ids);
  if (ids.length > 0) return ids;
  const threadId = focusedThreadId();
  return threadId ? cachedThreadMessageIds(threadId) : [];
}

const visible = or(withSelection(1), withFocusedThread());

export const mailboxActions: Action[] = [
  {
    id: "mail.undo",
    label: "Undo last action",
    description: "Reverse the most recent archive/trash/label mutation (daemon window ~60s)",
    group: "Mail",
    icon: Undo2,
    shortcut: "KeyZ",
    run: () => {
      const undo = useUndo.getState();
      // A pending undo-send window outranks mutation undo — it's the
      // most recent reversible action.
      if (undo.pendingSendCancel) {
        undo.pendingSendCancel();
        return;
      }
      const mutationId = undo.lastMutationId;
      const qc = getActiveQueryClient();
      if (!mutationId || !qc) {
        toast.info("Nothing to undo");
        return;
      }
      useModals.getState().setCommandPaletteOpen(false);
      void performUndo(qc, mutationId);
    },
  },
  {
    id: "mail.label",
    label: "Apply label",
    description: "Tag the focused thread or selected messages with a label",
    group: "Mail",
    icon: Tag,
    paletteOnly: true,
    when: visible,
    run: () => {
      const messageIds = targetMessageIds();
      if (messageIds.length === 0) {
        toast.error("Select messages or open a thread first");
        return;
      }
      useModals.getState().setCommandPaletteOpen(false);
      useModals.getState().openRightRail("label-picker", { mode: "label-add", messageIds });
    },
  },
  {
    id: "mail.move",
    label: "Move to label",
    description: "Move the focused thread or selected messages",
    group: "Mail",
    icon: ArrowRightCircle,
    paletteOnly: true,
    when: visible,
    run: () => {
      const messageIds = targetMessageIds();
      if (messageIds.length === 0) {
        toast.error("Select messages or open a thread first");
        return;
      }
      useModals.getState().setCommandPaletteOpen(false);
      useModals.getState().openRightRail("move-picker", { messageIds });
    },
  },
  {
    id: "mail.route",
    label: "Route from queue",
    description: "Apply a target label, remove the current queue label, mark read, and archive",
    group: "Mail",
    icon: RouteIcon,
    paletteOnly: true,
    when: visible,
    run: () => {
      const messageIds = targetMessageIds();
      const fromQueueLabel = activeRouteQueueLabel();
      if (messageIds.length === 0) {
        toast.error("Select messages or open a thread first");
        return;
      }
      if (!fromQueueLabel) {
        toast.error("Open a label queue before routing");
        return;
      }
      useModals.getState().setCommandPaletteOpen(false);
      useModals
        .getState()
        .openRightRail("route-picker", { messageIds, fromQueueLabel, archive: true });
    },
  },
  {
    id: "mail.read-and-archive",
    label: "Mark read and archive",
    description: "Combine mark-read + archive in one action",
    group: "Mail",
    icon: Archive,
    paletteOnly: true,
    when: visible,
    run: () => {
      const messageIds = targetMessageIds();
      if (messageIds.length === 0) {
        toast.error("Select messages or open a thread first");
        return;
      }
      useModals.getState().setCommandPaletteOpen(false);
      readAndArchiveMessages(messageIds)
        .then(() => toast.success(`Marked read and archived ${messageIds.length}`))
        .catch((error: Error) =>
          toast.error("Mark-read-and-archive failed", { description: error.message }),
        );
    },
  },
  {
    id: "mail.draft-assist",
    label: "Draft assist",
    description: "Generate a reply body for the focused thread via LLM",
    group: "Compose",
    paletteOnly: true,
    when: withFocusedThread(),
    run: () => {
      const threadId = focusedThreadId();
      if (!threadId) {
        toast.error("Open a thread first");
        return;
      }
      useModals.getState().setCommandPaletteOpen(false);
      useModals.getState().openRightRail("draft-assist", { threadId });
    },
  },
  {
    id: "mail.unsubscribe",
    label: "Unsubscribe from sender",
    description: "Send a list-unsubscribe request for the focused message",
    group: "Mail",
    icon: MailX,
    paletteOnly: true,
    when: withFocusedThread(),
    run: () => {
      const ids = targetMessageIds();
      if (ids.length === 0) {
        toast.error("Open a loaded thread first");
        return;
      }
      useModals.getState().setCommandPaletteOpen(false);
      const firstId = ids[0];
      if (!firstId) {
        toast.error("No message id");
        return;
      }
      unsubscribeFromSender({ messageId: firstId, archive: false })
        .then(() => toast.success("Unsubscribe requested"))
        .catch((error: Error) =>
          toast.error("Unsubscribe failed", { description: error.message }),
        );
    },
  },
  {
    id: "mail.unsubscribe-clear-sender",
    label: "Unsubscribe & clear sender",
    description: "Unsubscribe, then mark read and archive the sender's full footprint",
    group: "Mail",
    icon: MailX,
    paletteOnly: true,
    when: withFocusedThread(),
    run: () => {
      const sender = focusedSender();
      if (!sender) {
        toast.error("Open a loaded thread with a sender email first");
        return;
      }
      useModals.getState().setCommandPaletteOpen(false);
      unsubscribeAndClearSender({ address: sender.address, accountId: sender.accountId })
        .then((response) => {
          const result = response.result;
          if (!response.ok || !result) {
            toast.error("Unsubscribe & clear failed", { description: result?.error ?? "No result" });
            return;
          }
          toast.success(`Cleared ${result.archived_count} message(s) from ${result.address}`, {
            description: result.mutation_id ? `Undo id: ${result.mutation_id}` : undefined,
          });
        })
        .catch((error: Error) =>
          toast.error("Unsubscribe & clear failed", { description: error.message }),
        );
    },
  },
];
