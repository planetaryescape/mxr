import { toast } from "sonner";
import { create } from "zustand";

import { UNDO_TOAST_DURATION_MS } from "@/components/ui/sonner";
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
  duePhrase: string | null;
  resolution: TimeResolution;
  choice: TimeChoice;
  provenance: AiProvenance | null;
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

function sendCompleted(sendId: string, messageId: string | undefined): void {
  usePromiseOffers.setState((state) => ({
    sends: { ...state.sends, [sendId]: { status: "sent", messageId } },
  }));
  for (const offer of usePromiseOffers.getState().offers) {
    if (offer.sendId === sendId && offer.accepted) void keep(offer, messageId);
  }
}

function sendAbandoned(sendId: string): void {
  usePromiseOffers.setState((state) => ({
    sends: { ...state.sends, [sendId]: { status: "gone" } },
    offers: state.offers.filter((offer) => offer.sendId !== sendId),
  }));
}

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
    duration: UNDO_TOAST_DURATION_MS,
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

function refreshPromiseViews(): void {
  void invalidateMailQueries();
  void getActiveQueryClient()?.invalidateQueries({ queryKey: ["commitments"] });
}

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
