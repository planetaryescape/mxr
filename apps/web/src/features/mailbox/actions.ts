/*
 * Mailbox-side palette actions: label-add, label-remove, move, read-and-archive,
 * unsubscribe. Visible when the user has either a focused thread or a non-empty
 * selection. Runners read selection from the store at invocation time.
 *
 * The actual API mutations route through `useOptimisticMailMutation` inside
 * the picker components — these registry runners just open the right panel.
 */

import { Archive, ArrowRightCircle, Tag, MailX } from "lucide-react";
import { toast } from "sonner";

import {
  readAndArchiveMessages,
  unsubscribeFromSender,
} from "@/features/mailbox/api";
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

function cachedThreadMessageIds(threadId: string): string[] {
  const cached = getActiveQueryClient()?.getQueryData<ThreadResponse>(["thread", threadId]);
  return cached?.messages.map((message) => message.id) ?? [];
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
];
