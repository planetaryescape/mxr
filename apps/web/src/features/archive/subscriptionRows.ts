import type { RecordSubscription } from "./api";
import { longDate } from "./ledger";

export function whenLabel(subscription: RecordSubscription): string {
  if (subscription.status === "overdue" && subscription.next_expected)
    return `overdue since ${longDate(subscription.next_expected)}`;
  if (subscription.status === "ended" || !subscription.next_expected)
    return `ended, last ${longDate(subscription.last_charge)}`;
  return `next ${longDate(subscription.next_expected)}`;
}

export function splitLive(subscriptions: readonly RecordSubscription[]) {
  return {
    live: subscriptions.filter((s) => s.status !== "ended"),
    ended: subscriptions.filter((s) => s.status === "ended"),
  };
}

export function subscriptionAmountUnchecked(subscription: RecordSubscription): boolean {
  return subscription.fields.some((field) => field.field === "amount" && !field.checked);
}

export function subscriptionAmountCopy(subscription: RecordSubscription): string | null {
  return subscription.fields.find((field) => field.field === "amount")?.copy ?? null;
}
