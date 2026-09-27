/*
 * What the keys in a place do, shared by Reading and Paper trail: pin,
 * sweep, move the sender, unsubscribe. Each opens the same dialog or runs
 * the same request whichever view asked.
 */

import { toast } from "sonner";

import { openMailDialog } from "@/features/mail-actions/mailDialogStore";
import { targetFromRows } from "@/features/mail-actions/target";
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

/** Flip the pinned flag in every cached copy of the place. */
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

/** Pin or unpin at once; the daemon confirms, a failure puts it back. */
export async function togglePin(message: PlaceMessage): Promise<void> {
  const pinned = !message.pinned;
  patchPinned(message.message_id, pinned);
  try {
    await pinMessages([message.message_id], pinned);
    toast.success(pinned ? "Pinned: a sweep leaves it here" : "Unpinned", { duration: 2500 });
  } catch (caught) {
    patchPinned(message.message_id, !pinned);
    toast.error(pinned ? "Couldn't pin" : "Couldn't unpin", {
      description: caught instanceof Error ? caught.message : String(caught),
    });
  }
}

/**
 * Open the sweep preview. `shownHere` is how many of the messages it covers
 * are on screen, so the dialog can say how many more it reaches.
 */
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

/** A place message as a mail row, for dialogs built around rows. */
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

/** The kind menu for the sender of a message the daemon has classified. */
export function openMoveSenderForKind(kind: MessageKindResponse, senderLabel: string): void {
  openMailDialog({
    kind: "sender-kind",
    accountId: kind.account_id,
    senderEmail: kind.sender_email,
    senderLabel: senderLabel || kind.sender_email,
    current: kind.mail_kind,
  });
}

/** From the reader: ask the daemon about the message, then offer the menu. */
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
