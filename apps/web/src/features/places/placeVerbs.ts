import { toast } from "sonner";

import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { targetFromRows } from "@/features/mail-actions/target";
import { toneFor } from "@/features/mail-actions/verbFeedback";
import type { MessageRowView } from "@/features/mailbox/types";
import { getActiveQueryClient } from "@/lib/queryClient";

import {
  messageKindQuery,
  pinMessages,
  type MessageKindResponse,
  type Place,
  type PlaceBundle,
  type PlaceMessage,
} from "./api";
import { bundleSender } from "./placeCopy";
import { mapPlaceData } from "./placePaging";

function patchPinned(messageId: string, pinned: boolean) {
  getActiveQueryClient()?.setQueriesData({ queryKey: ["place"] }, (data: unknown) =>
    mapPlaceData(data, (page) => ({
      ...page,
      bundles: page.bundles.map((bundle) => {
        if (!bundle.messages.some((message) => message.message_id === messageId)) return bundle;
        return {
          ...bundle,
          pinned_count: Math.max(0, bundle.pinned_count + (pinned ? 1 : -1)),
          messages: bundle.messages.map((message) =>
            message.message_id === messageId ? { ...message, pinned } : message,
          ),
        };
      }),
    })),
  );
}

export async function togglePin(message: PlaceMessage): Promise<void> {
  const pinned = !message.pinned;
  patchPinned(message.message_id, pinned);
  try {
    await pinMessages([message.message_id], pinned);
    toast[toneFor("pin")](pinned ? "Pinned: a sweep leaves it here" : "Unpinned", {
      duration: 2500,
    });
  } catch (caught) {
    patchPinned(message.message_id, !pinned);
    toast.error(pinned ? "Couldn't pin" : "Couldn't unpin", {
      description: caught instanceof Error ? caught.message : String(caught),
    });
  }
}

export function openSweep(
  place: Place,
  accountId: string | null,
  bundle?: PlaceBundle,
  shownHere?: number,
): void {
  openMailDialog({
    kind: "sweep",
    scope: bundle
      ? { place, accountId: bundle.account_id, senderEmail: bundle.sender_email }
      : { place, accountId },
    senderLabel: bundle ? bundleSender(bundle) : undefined,
    shownHere,
  });
}

export function openMoveSender(bundle: PlaceBundle): void {
  openMailDialog({
    kind: "sender-kind",
    accountId: bundle.account_id,
    senderEmail: bundle.sender_email,
    senderLabel: bundleSender(bundle),
    current: bundle.kind,
  });
}

export function placeMessageRow(bundle: PlaceBundle, message: PlaceMessage): MessageRowView {
  return {
    id: message.message_id,
    kind: "message",
    account_id: bundle.account_id,
    thread_id: message.thread_id,
    provider_id: "",
    sender: bundleSender(bundle),
    sender_detail: bundle.sender_email,
    subject: message.subject,
    snippet: message.snippet,
    date: message.date,
    date_label: "",
    date_full: message.date,
    date_relative: "",
    unread: message.unread,
    starred: message.starred,
    has_attachments: false,
  };
}

export function openUnsubscribe(bundle: PlaceBundle, message: PlaceMessage): void {
  openMailDialog({
    kind: "unsubscribe",
    target: targetFromRows([placeMessageRow(bundle, message)], "list"),
  });
}

export function openMoveSenderForKind(kind: MessageKindResponse, senderLabel: string): void {
  openMailDialog({
    kind: "sender-kind",
    accountId: kind.account_id,
    senderEmail: kind.sender_email,
    senderLabel: senderLabel || kind.sender_email,
    current: kind.mail_kind,
  });
}

export async function openMoveSenderFor(messageId: string, senderLabel: string): Promise<void> {
  const query = messageKindQuery(messageId);
  try {
    const kind = await (getActiveQueryClient()?.fetchQuery(query) ?? query.queryFn());
    openMoveSenderForKind(kind, senderLabel);
  } catch (caught) {
    toast.error("Couldn't tell where this sender belongs", {
      description: caught instanceof Error ? caught.message : String(caught),
    });
  }
}
