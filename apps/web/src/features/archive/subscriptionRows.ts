/*
 * Archive's subscriptions, shaped for drawing. The daemon detects them and
 * works out every amount, date and total; these only pick words.
 */

import type { RecordSubscription } from "./api";
import { longDate } from "./ledger";

/** "next 3 Jul 2025", "overdue since 3 May 2025", "ended, last 3 Apr 2025". */
export function whenLabel(subscription: RecordSubscription): string {
  if (subscription.status === "overdue" && subscription.next_expected)
    return `overdue since ${longDate(subscription.next_expected)}`;
  if (subscription.status === "ended" || !subscription.next_expected)
    return `ended, last ${longDate(subscription.last_charge)}`;
  return `next ${longDate(subscription.next_expected)}`;
}

/** Live (active and overdue) and ended, each in the daemon's order. */
export function splitLive(subscriptions: readonly RecordSubscription[]) {
  return {
    live: subscriptions.filter((s) => s.status !== "ended"),
    ended: subscriptions.filter((s) => s.status === "ended"),
  };
}

/** The amount came from a rule nobody has confirmed. */
export function subscriptionAmountUnchecked(subscription: RecordSubscription): boolean {
  return subscription.fields.some((field) => field.field === "amount" && !field.checked);
}

/** What `Y` copies: the amount as a plain number. */
export function subscriptionAmountCopy(subscription: RecordSubscription): string | null {
  return subscription.fields.find((field) => field.field === "amount")?.copy ?? null;
}
