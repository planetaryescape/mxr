/*
 * "You promised: send the deck. Remind me Friday 2 October, 09:00?" Offered
 * beside a send, never in its way: the message goes out on its own undo
 * window whatever the answer, and an answer given early is kept once it has
 * gone. The time is the parser's reading of the words in the message and
 * can be changed in words before it is kept.
 */

import { BellRing, X } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { describeChoice } from "@/features/time/api";
import { NaturalTimeInput } from "@/features/time/NaturalTimeInput";
import { useNaturalTime } from "@/features/time/useNaturalTime";

import { acceptOffer, dismissOffer, usePromiseOffers, type PromiseOffer } from "./promiseOffers";

export function PromiseTray() {
  const offers = usePromiseOffers((state) => state.offers);
  if (offers.length === 0) return null;
  return (
    // Bottom-left of the content, just past the sidebar (never over it),
    // clear of the reply's Send button and of the send countdown in the
    // bottom-right corner.
    <section
      aria-label="Promises in what you sent"
      data-testid="promise-tray"
      className="pointer-events-none fixed inset-x-4 top-14 z-40 flex flex-col gap-2 sm:inset-x-auto sm:bottom-11 sm:left-[calc(var(--shell-sidebar-w)+1rem)] sm:top-auto sm:w-[26rem] [[data-sidebar-collapsed=true]_&]:sm:left-[calc(var(--shell-sidebar-collapsed-w)+1rem)]"
    >
      {offers.map((offer) => (
        <PromiseCard key={offer.id} offer={offer} />
      ))}
    </section>
  );
}

function PromiseCard({ offer }: { offer: PromiseOffer }) {
  const [changing, setChanging] = useState(false);
  const time = useNaturalTime({ enabled: changing });
  const when = describeChoice(offer.choice);

  function startChange() {
    time.setValue(offer.resolution.input);
    setChanging(true);
  }

  return (
    <div
      role="group"
      aria-label={`You promised: ${offer.what}`}
      data-testid="promise-offer"
      className="pointer-events-auto w-full rounded-md border border-border-strong bg-popover px-3.5 py-3 text-popover-foreground shadow-xl animate-in fade-in-0 duration-base ease-out"
    >
      <div className="flex items-start gap-2.5">
        <BellRing className="mt-0.5 size-4 shrink-0 text-primary" aria-hidden />
        <div className="min-w-0 flex-1">
          <p className="text-pretty text-[13px] leading-5">
            <span className="text-muted-foreground">You promised:</span>{" "}
            <span className="font-medium">{offer.what}</span>
            {offer.duePhrase ? (
              <span className="text-muted-foreground"> ({offer.duePhrase})</span>
            ) : null}
          </p>
          {offer.accepted ? (
            <p className="mt-1 text-xs tabular-nums text-muted-foreground" role="status">
              Reminder for <span className="text-foreground">{when}</span>, kept once it sends.
            </p>
          ) : changing ? (
            <div className="mt-2">
              <NaturalTimeInput
                id={`${offer.id}-time`}
                state={time}
                autoFocus
                placeholder="When should mxr remind you?"
                onCommit={(choice) => acceptOffer(offer.id, choice)}
              />
            </div>
          ) : (
            <p className="mt-1 text-xs tabular-nums" data-testid="promise-offer-time">
              Remind me <span className="font-medium text-foreground">{when}</span>?
            </p>
          )}
          {offer.provenance ? (
            <p className="mt-1 font-mono text-2xs text-muted-foreground">
              {offer.provenance.locality === "local" ? "Local model" : "Cloud model"}{" "}
              {offer.provenance.model} · from your message
            </p>
          ) : null}
        </div>
        <button
          type="button"
          onClick={() => dismissOffer(offer.id)}
          aria-label="Not now"
          title="Not now"
          className="-mr-1 shrink-0 rounded p-0.5 text-muted-foreground hover:text-foreground"
        >
          <X className="size-3.5" />
        </button>
      </div>
      {offer.accepted ? null : (
        <div className="mt-2.5 flex items-center justify-end gap-1.5">
          {changing ? (
            <Button size="sm" variant="ghost" onClick={() => setChanging(false)}>
              Back
            </Button>
          ) : (
            <Button size="sm" variant="ghost" onClick={startChange}>
              Change time
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={() => dismissOffer(offer.id)}>
            Not now
          </Button>
          <Button
            size="sm"
            disabled={changing && !time.canCommit}
            onClick={() =>
              changing
                ? void time.commit((choice) => acceptOffer(offer.id, choice))
                : acceptOffer(offer.id)
            }
          >
            Remind me
          </Button>
        </div>
      )}
    </div>
  );
}
