import { Copy, Mail } from "lucide-react";
import { memo } from "react";

import { KeyChip } from "@/components/KeyChip";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";

import type { RecordSubscription, RecordSubscriptions } from "./api";
import { longDate } from "./ledger";
import { OpenDot, Provenance } from "./RecordParts";
import { splitLive, subscriptionAmountUnchecked, whenLabel } from "./subscriptionRows";

/** One subscription: what, amount, how often, the next charge and status. */
const SubscriptionRow = memo(function SubscriptionRow({
  subscription,
  index,
  focused,
  onSelect,
}: {
  subscription: RecordSubscription;
  index: number;
  focused: boolean;
  onSelect: (subscription: RecordSubscription) => void;
}) {
  const ended = subscription.status === "ended";
  return (
    <li
      data-index={index}
      data-testid="subscription-row"
      data-id={subscription.id}
      data-status={subscription.status}
      aria-current={focused ? "true" : undefined}
      className={cn(
        "mx-2 cursor-default rounded-md px-3 py-2 text-[13px]",
        focused ? "bg-accent" : "hover:bg-accent/60",
      )}
      onClick={() => onSelect(subscription)}
    >
      <div className="grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-3 @2xl:grid-cols-[minmax(0,1fr)_7.5rem_6rem_12rem]">
        <span
          className={cn(
            "min-w-0 truncate font-medium",
            ended ? "text-muted-foreground" : "text-foreground",
          )}
        >
          {subscription.title}
          {subscription.confirmed ? null : (
            <span className="font-normal text-muted-foreground"> · seen twice</span>
          )}
        </span>
        <span
          data-testid="subscription-amount"
          className="inline-flex items-center justify-end gap-1.5 font-mono tabular-nums text-foreground"
        >
          {subscription.amount && subscriptionAmountUnchecked(subscription) ? (
            <OpenDot label="amount unchecked" />
          ) : null}
          {subscription.amount?.display ?? ""}
        </span>
        <span className="hidden text-muted-foreground @2xl:block">
          {subscription.cadence_label}
        </span>
        <span
          data-testid="subscription-when"
          className={cn(
            "col-span-2 text-[12.5px] @2xl:col-span-1 @2xl:text-right",
            subscription.status === "overdue" ? "text-warning" : "text-muted-foreground",
          )}
        >
          <span className="@2xl:hidden">{subscription.cadence_label} · </span>
          {whenLabel(subscription)}
        </span>
      </div>
    </li>
  );
});

/**
 * The Subscriptions view: what live subscriptions cost per currency, what
 * changed, then one row per subscription, live before ended.
 */
export function SubscriptionsList({
  data,
  cursorId,
  onSelect,
}: {
  data: RecordSubscriptions;
  cursorId: string | null;
  onSelect: (subscription: RecordSubscription) => void;
}) {
  const { live, ended } = splitLive(data.subscriptions);
  const rows = (list: RecordSubscription[], offset: number) => (
    <ul className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
      {list.map((subscription, at) => (
        <SubscriptionRow
          key={subscription.id}
          subscription={subscription}
          index={offset + at}
          focused={subscription.id === cursorId}
          onSelect={onSelect}
        />
      ))}
    </ul>
  );
  return (
    <section aria-label="Subscriptions" data-testid="subscriptions" className="mt-3">
      <p className="mx-5 text-[12.5px] text-muted-foreground">{data.header}</p>
      {data.totals.length > 0 ? (
        <p data-testid="subscription-totals" className="mx-5 mt-2 text-[13px] text-foreground">
          <span className="font-medium">{plural(data.live, "live subscription")}</span>
          {data.totals.map((total) => (
            <span key={total.currency} className="ml-3 font-mono tabular-nums">
              {total.per_month.display} a month · {total.per_year.display} a year
            </span>
          ))}
        </p>
      ) : null}
      {data.signals && data.signals.length > 0 ? (
        <ul
          aria-label="Worth a look"
          data-testid="subscription-signals"
          className="mx-5 mt-2 grid gap-0.5 text-[13px]"
        >
          {data.signals.map((signal) => (
            <li key={`${signal.kind}-${signal.subscription_id}`} className="text-foreground">
              {signal.label}
            </li>
          ))}
        </ul>
      ) : null}
      {data.empty_state ? (
        <p
          data-testid="subscriptions-empty"
          className="mx-5 mt-4 text-[13px] text-muted-foreground"
        >
          {data.empty_state}
        </p>
      ) : null}
      {live.length > 0 ? <div className="mt-3">{rows(live, 0)}</div> : null}
      {ended.length > 0 ? (
        <section aria-label="Ended" className="mt-4">
          <h2 className="mx-5 mb-1 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
            Ended
          </h2>
          {rows(ended, live.length)}
        </section>
      ) : null}
    </section>
  );
}

/**
 * A subscription's card: its fields with where they came from, the price
 * changes, and every charge, newest first.
 */
export function SubscriptionCard({
  subscription,
  onCopyAmount,
  onOpenEmail,
  onIssuer,
}: {
  subscription: RecordSubscription;
  onCopyAmount: () => void;
  onOpenEmail: () => void;
  onIssuer: () => void;
}) {
  return (
    <article
      aria-label="Subscription"
      data-testid="subscription-card"
      className="grid gap-3 px-5 py-4"
    >
      <header>
        <p className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
          {subscription.cadence_label} · {subscription.status}
        </p>
        <h2 className="mt-1 text-[16px] font-semibold text-foreground">{subscription.title}</h2>
        <p className="text-[13px] text-foreground/90">{subscription.status_reason}</p>
      </header>
      <dl className="text-[13px]">
        {subscription.fields
          .filter((field) => field.field !== "status")
          .map((field) => (
            <div
              key={field.field}
              className="grid grid-cols-[7rem_minmax(0,1fr)] items-baseline gap-x-3 py-1"
            >
              <dt className="text-[12px] text-muted-foreground">{field.label}</dt>
              <dd className="min-w-0">
                <span className="inline-flex flex-wrap items-center gap-x-3 gap-y-0.5">
                  <span className="font-mono tabular-nums text-foreground">{field.value}</span>
                  {field.field === "amount" ? (
                    <button
                      type="button"
                      onClick={onCopyAmount}
                      aria-label="Copy amount (Y)"
                      className="inline-flex items-center gap-1 rounded px-1 text-[12px] text-muted-foreground hover:bg-accent hover:text-foreground"
                    >
                      <Copy aria-hidden className="size-3" />
                      <KeyChip className="h-4 px-1">Y</KeyChip>
                    </button>
                  ) : null}
                  <Provenance field={field} />
                </span>
              </dd>
            </div>
          ))}
      </dl>
      {subscription.price_changes && subscription.price_changes.length > 0 ? (
        <section aria-label="Price changes">
          <h3 className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
            Price changes
          </h3>
          <ul data-testid="price-changes" className="mt-1 grid gap-0.5 text-[13px]">
            {subscription.price_changes.map((change) => (
              <li key={change.record_id} className="font-mono tabular-nums">
                {change.label}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      <section aria-label="Charges">
        <h3 className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
          {plural(subscription.charge_count, "charge")}
        </h3>
        <ul data-testid="subscription-charges" className="mt-1 grid gap-0.5 text-[12.5px]">
          {subscription.charges.toReversed().map((charge) => (
            <li key={charge.record_ids[0]} className="flex items-center gap-3">
              <span className="w-24 shrink-0 font-mono tabular-nums text-muted-foreground">
                {longDate(charge.date)}
              </span>
              <span className="inline-flex items-center gap-1.5 font-mono tabular-nums">
                {charge.checked ? null : <OpenDot />}
                {charge.amount?.display ?? "no amount"}
              </span>
            </li>
          ))}
        </ul>
      </section>
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={onOpenEmail}
          className="inline-flex min-h-10 items-center gap-2 rounded-md border border-border bg-background px-3 text-[13px] hover:bg-accent"
        >
          <Mail aria-hidden className="size-4" />
          The latest email
          <KeyChip className="h-4 px-1">o</KeyChip>
        </button>
      </div>
      <button
        type="button"
        onClick={onIssuer}
        className="w-fit text-left text-[12.5px] text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
      >
        Every record from {subscription.issuer}
        {subscription.one_offs > 0 ? ` (${subscription.one_offs} not part of this)` : ""}{" "}
        <KeyChip className="h-4 px-1">p</KeyChip>
      </button>
      <p data-testid="subscription-why" className="text-[12.5px] text-muted-foreground">
        {subscription.why}
      </p>
    </article>
  );
}
