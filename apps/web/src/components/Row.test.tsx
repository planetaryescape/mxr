/* @vitest-environment jsdom */

import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import type { RuntimeAccount } from "@/features/compose/api";
import { AccountIdentitiesContext, identitiesFrom } from "@/features/accounts/useAccountIdentities";
import { LANDING_MS, useLanding } from "@/state/landingStore";

import { Row, RowVerb } from "./Row";

/** Two enabled accounts with identities: the view mixes accounts unless scoped. */
const ACCOUNTS: RuntimeAccount[] = [
  {
    account_id: "work",
    name: "Work",
    email: "bk@work.example",
    provider_kind: "imap",
    is_default: true,
    enabled: true,
    identity: { color: "sky", tag: "W" },
  },
  {
    account_id: "home",
    name: "Home",
    email: "bk@home.example",
    provider_kind: "imap",
    is_default: false,
    enabled: true,
    identity: { color: "amber", tag: "H" },
  },
];

function renderRow(
  props: Partial<React.ComponentProps<typeof Row>> = {},
  identities = identitiesFrom([], false),
) {
  return render(
    <AccountIdentitiesContext.Provider value={identities}>
      <ul>
        <Row
          lit={false}
          primary="Pat Colleague"
          secondary="RustConf invite"
          trail={<time>2h</time>}
          verbs={<RowVerb keys="r" label="reply" onClick={() => {}} />}
          data-testid="row"
          {...props}
        />
      </ul>
    </AccountIdentitiesContext.Provider>,
  );
}

describe("Row", () => {
  test("an unread row names the state for assistive tech", () => {
    const { rerender } = renderRow({ unread: true });
    expect(screen.getByText("Unread.")).toHaveClass("sr-only");
    rerender(
      <ul>
        <Row lit={false} unread={false} primary="Pat" data-testid="row" />
      </ul>,
    );
    expect(screen.queryByText("Unread.")).toBeNull();
  });

  test("a verb is its own act: it doesn't also light the row", () => {
    const onRow = vi.fn<() => void>();
    const onVerb = vi.fn<() => void>();
    renderRow({
      lit: true,
      onClick: onRow,
      verbs: <RowVerb keys="r" label="reply" onClick={onVerb} />,
    });
    fireEvent.click(screen.getByRole("button", { name: "reply" }));
    expect(onVerb).toHaveBeenCalledOnce();
    expect(onRow).not.toHaveBeenCalled();
  });

  test("a checkbox lead sits beside the row's link, not inside it", () => {
    render(
      <ul>
        <Row
          lit={false}
          leadControl
          lead={<button type="button">tick</button>}
          primary="Pay the invoice"
          renderMain={(content, className) => (
            <a href="/todo" className={className}>
              {content}
            </a>
          )}
        />
      </ul>,
    );
    expect(screen.getByRole("link")).not.toContainElement(screen.getByRole("button"));
  });

  describe("landing", () => {
    afterEach(() => {
      vi.useRealTimers();
      act(() => useLanding.setState({ landed: new Set() }));
    });

    test("a change that lands on one of its ids underlines the row for 600ms", () => {
      vi.useFakeTimers();
      renderRow({ land: ["thread-1", "msg-1"] });
      expect(screen.getByTestId("row")).not.toHaveClass("landed");
      act(() => useLanding.getState().land(["msg-1"]));
      expect(screen.getByTestId("row")).toHaveClass("landed");
      act(() => vi.advanceTimersByTime(LANDING_MS));
      expect(screen.getByTestId("row")).not.toHaveClass("landed");
    });

  });

  describe("account mark (docs/blueprint/33)", () => {
    test("a row from a mixed view carries its account's chip before the time", () => {
      renderRow({ accountId: "work", time: <span>10:02</span> }, identitiesFrom(ACCOUNTS, false));
      const chip = screen.getByRole("img", { name: "Work" });
      expect(chip).toHaveTextContent("W");
      const time = screen.getByText("10:02");
      expect(chip.compareDocumentPosition(time) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    test("no mark for a row whose account is unknown, or for a one-account user", () => {
      const { unmount } = renderRow({ accountId: "nobody" }, identitiesFrom(ACCOUNTS, false));
      expect(screen.queryByRole("img")).toBeNull();
      unmount();
      renderRow({ accountId: "work" }, identitiesFrom(ACCOUNTS.slice(0, 1), false));
      expect(screen.queryByRole("img")).toBeNull();
    });

  });
});
