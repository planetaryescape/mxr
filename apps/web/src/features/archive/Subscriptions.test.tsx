import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { RecordSubscription } from "./api";
import { SubscriptionCard, SubscriptionsList } from "./Subscriptions";
import {
  splitLive,
  subscriptionAmountCopy,
  subscriptionAmountUnchecked,
  whenLabel,
} from "./subscriptionRows";
import { subscriptionFixture, subscriptionsFixture } from "./testing";

describe("Archive's subscriptions", () => {
  test("the when column says next, overdue or ended in words", () => {
    expect(whenLabel(subscriptionFixture())).toBe("next 10 May 2025");
    expect(whenLabel(subscriptionFixture({ status: "overdue" }))).toBe("overdue since 10 May 2025");
    expect(whenLabel(subscriptionFixture({ status: "ended", next_expected: null }))).toBe(
      "ended, last 10 Apr 2025",
    );
  });

  test("ended subscriptions are drawn apart from live ones", () => {
    const { live, ended } = splitLive(subscriptionsFixture().subscriptions);
    expect(live.map((s) => s.id)).toEqual(["sub_netflix"]);
    expect(ended.map((s) => s.id)).toEqual(["sub_disney"]);
  });

  test("a rule's amount is unchecked and Y copies it as a plain number", () => {
    const netflix = subscriptionFixture();
    expect(subscriptionAmountUnchecked(netflix)).toBe(true);
    expect(subscriptionAmountCopy(netflix)).toBe("12.99");
  });

  test("the list leads with totals per currency and what changed, then live, then ended", () => {
    const onSelect = vi.fn<(subscription: RecordSubscription) => void>();
    render(
      <SubscriptionsList
        data={subscriptionsFixture()}
        cursorId="sub_netflix"
        onSelect={onSelect}
      />,
    );
    expect(screen.getByTestId("subscription-totals")).toHaveTextContent(
      "£12.99 a month · £155.88 a year",
    );
    expect(screen.getByTestId("subscription-signals")).toHaveTextContent(
      "Netflix went up from £10.99 to £12.99",
    );
    const rows = screen.getAllByTestId("subscription-row");
    expect(rows.map((row) => row.dataset.id)).toEqual(["sub_netflix", "sub_disney"]);
    expect(rows[0]).toHaveAttribute("aria-current", "true");
    expect(within(rows[0]!).getByTestId("subscription-when")).toHaveTextContent("next 10 May 2025");
    fireEvent.click(rows[1]!);
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ id: "sub_disney" }));
  });

  test("no subscriptions shows the daemon's words for how they appear", () => {
    render(
      <SubscriptionsList
        data={subscriptionsFixture({
          subscriptions: [],
          totals: [],
          signals: [],
          live: 0,
          ended: 0,
          empty_state: "No subscriptions yet.",
        })}
        cursorId={null}
        onSelect={vi.fn<(subscription: RecordSubscription) => void>()}
      />,
    );
    expect(screen.getByTestId("subscriptions-empty")).toHaveTextContent("No subscriptions yet.");
    expect(screen.queryByTestId("subscription-row")).toBeNull();
  });

  test("the card shows provenance, the price change and each charge newest first", () => {
    const onCopyAmount = vi.fn<() => void>();
    const onIssuer = vi.fn<() => void>();
    render(
      <SubscriptionCard
        subscription={subscriptionFixture()}
        onCopyAmount={onCopyAmount}
        onOpenEmail={vi.fn<() => void>()}
        onIssuer={onIssuer}
      />,
    );
    const card = screen.getByTestId("subscription-card");
    expect(card).toHaveTextContent("from a pattern in the email · unchecked");
    expect(card).toHaveTextContent("from your 4 charges · unchecked");
    expect(screen.getByTestId("price-changes")).toHaveTextContent(
      "£10.99 to £12.99 on 10 Apr 2025",
    );
    const charges = within(screen.getByTestId("subscription-charges")).getAllByRole("listitem");
    expect(charges[0]).toHaveTextContent("£12.99");
    expect(charges[1]).toHaveTextContent("£10.99");
    fireEvent.click(screen.getByRole("button", { name: "Copy amount (Y)" }));
    expect(onCopyAmount).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /Every record from Netflix/ }));
    expect(onIssuer).toHaveBeenCalled();
  });
});
