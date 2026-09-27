/*
 * Promises found in outgoing mail, offered as reminders while the send's
 * undo window runs. The send never waits: the check runs beside it, and an
 * answer given before the message has gone out is held until it has (it
 * needs the sent message's id). Undoing or failing the send takes its
 * offers with it, so nothing is kept for mail that never went out.
 */

import { toast } from "sonner";
import { create } from "zustand";

import { resolveCommitment } from "@/features/mailbox/api";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { onSendEvent } from "@/features/compose/session/sendEvents";
import { errorMessage } from "@/features/compose/session/composeDraft";
import { describeChoice, type TimeChoice, type TimeResolution } from "@/features/time/api";
import { getActiveQueryClient } from "@/lib/queryClient";

import { recordPromise, type PromiseDetection } from "./api";

type AiProvenance = NonNullable<PromiseDetection["provenance"]>;

export interface PromiseOffer {
  id: string;
  sendId: string;
  what: string;
  /** The due words as the message wrote them ("by Friday"). */
  duePhrase: string | null;
  /** The parser's reading of them, for "Change" to start from. */
  resolution: TimeResolution;
  /** The time the reminder is for: the default reading until changed. */
  choice: TimeChoice;
  provenance: AiProvenance | null;
  /** Accepted while the send was still in its undo window. */
  accepted: boolean;
}

type SendState =
  | { status: "pending" }
  | { status: "sent"; messageId?: string }
  | { status: "gone" };

interface PromiseOffersState {
  offers: PromiseOffer[];
  sends: Record<string, SendState>;
}

export const usePromiseOffers = create<PromiseOffersState>(() => ({ offers: [], sends: {} }));

let nextId = 0;

/** Turn a detection into offers, one per promise with a date. Ignored once
 * the send is gone, and for anything other than a ready answer. */
export function offerPromises(sendId: string, detection: PromiseDetection): void {
  const send = usePromiseOffers.getState().sends[sendId];
  if (!send || send.status === "gone" || detection.status !== "ready") return;
  const offers: PromiseOffer[] = [];
  for (const promise of detection.promises) {
    const choice = promise.due?.choices[0];
    if (!promise.due || !choice) continue;
    nextId += 1;
    offers.push({
      id: `promise-${nextId}`,
      sendId,
      what: promise.what,
      duePhrase: promise.due_phrase ?? null,
      resolution: promise.due,
      choice,
      provenance: detection.provenance ?? null,
      accepted: false,
    });
  }
  if (offers.length === 0) return;
  usePromiseOffers.setState((state) => ({ offers: [...state.offers, ...offers] }));
}

/** The message went out: keep every promise already accepted for it. */
function sendCompleted(sendId: string, messageId: string | undefined): void {
  usePromiseOffers.setState((state) => ({
    sends: { ...state.sends, [sendId]: { status: "sent", messageId } },
  }));
  for (const offer of usePromiseOffers.getState().offers) {
    if (offer.sendId === sendId && offer.accepted) void keep(offer, messageId);
  }
}

/** Undone or failed: nothing was sent, so nothing is offered or kept. */
function sendAbandoned(sendId: string): void {
  usePromiseOffers.setState((state) => ({
    sends: { ...state.sends, [sendId]: { status: "gone" } },
    offers: state.offers.filter((offer) => offer.sendId !== sendId),
  }));
}

/** "Remind me", at the offer's time or at `choice` when the user changed it. */
export function acceptOffer(offerId: string, choice?: TimeChoice): void {
  const offer = findOffer(offerId);
  if (!offer) return;
  const accepted = { ...offer, choice: choice ?? offer.choice, accepted: true };
  const send = usePromiseOffers.getState().sends[offer.sendId];
  if (send?.status === "sent") {
    void keep(accepted, send.messageId);
    return;
  }
  replaceOffer(accepted);
}

export function dismissOffer(offerId: string): void {
  usePromiseOffers.setState((state) => ({
    offers: state.offers.filter((offer) => offer.id !== offerId),
  }));
}

async function keep(offer: PromiseOffer, messageId: string | undefined): Promise<void> {
  dismissOffer(offer.id);
  const label = describeChoice(offer.choice);
  if (!messageId) {
    toast.warning("Sent, but no reminder was set", {
      description: "The mxr bridge did not return the sent message id.",
    });
    return;
  }
  let commitmentId: string;
  try {
    commitmentId = (await recordPromise(messageId, offer.what, new Date(offer.choice.at))).id;
  } catch (error) {
    toast.error("Couldn't set the reminder", {
      description: errorMessage(error),
    });
    return;
  }
  refreshPromiseViews();
  toast.success(`Reminder set for ${label}`, {
    description: `You promised: ${offer.what}.`,
    duration: 10_000,
    action: {
      label: "Undo",
      onClick: () => {
        resolveCommitment(commitmentId)
          .then(() => {
            toast.success("Reminder removed");
            refreshPromiseViews();
          })
          .catch((error: unknown) =>
            toast.error("Couldn't remove the reminder", {
              description: errorMessage(error),
            }),
          );
      },
    },
  });
}

/** Thread context (under "thread") and commitment lists show promises. */
function refreshPromiseViews(): void {
  void invalidateMailQueries();
  void getActiveQueryClient()?.invalidateQueries({ queryKey: ["commitments"] });
}

// Offers follow the send they came from, whichever view started it.
onSendEvent((event) => {
  switch (event.kind) {
    case "queued":
      usePromiseOffers.setState((state) => ({
        sends: { ...state.sends, [event.sendId]: { status: "pending" } },
      }));
      break;
    case "sent":
      sendCompleted(event.sendId, event.sentMessageId);
      break;
    case "cancelled":
    case "failed":
      sendAbandoned(event.sendId);
      break;
  }
});

function findOffer(offerId: string): PromiseOffer | undefined {
  return usePromiseOffers.getState().offers.find((offer) => offer.id === offerId);
}

function replaceOffer(next: PromiseOffer): void {
  usePromiseOffers.setState((state) => ({
    offers: state.offers.map((offer) => (offer.id === next.id ? next : offer)),
  }));
}
