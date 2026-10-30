/* @vitest-environment jsdom */

import { act, fireEvent, render, screen, within } from "@testing-library/react";
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
  test("every row is the same fixed height, lit or not", () => {
    const { rerender } = renderRow();
    const inner = () => screen.getByTestId("row").firstElementChild;
    expect(inner()).toHaveClass("h-(--row-height)");
    rerender(
      <ul>
        <Row lit primary="Pat Colleague" data-testid="row" />
      </ul>,
    );
    expect(inner()).toHaveClass("h-(--row-height)");
  });

  test("lit draws the lamp surface and nothing else; unlit draws it transparent", () => {
    const { rerender } = renderRow();
    const layers = () => [...screen.getByTestId("row").querySelectorAll("span[aria-hidden]")];
    expect(layers().every((layer) => layer.classList.contains("opacity-0"))).toBe(true);
    expect(screen.getByTestId("row")).not.toHaveAttribute("data-lit");
    rerender(
      <ul>
        <Row lit primary="Pat" data-testid="row" />
      </ul>,
    );
    expect(screen.getByTestId("row")).toHaveAttribute("data-lit", "true");
    expect(layers().filter((layer) => layer.classList.contains("opacity-100"))).toHaveLength(1);
  });

  test("a key never fades the lamp; a pointer does", () => {
    const { rerender } = renderRow({ lit: true, via: "key" });
    const surface = () => screen.getByTestId("row").querySelector("span.bg-ink-lamp");
    expect(surface()).not.toHaveClass("transition-[opacity]");
    rerender(
      <ul>
        <Row lit via="pointer" primary="Pat" data-testid="row" />
      </ul>,
    );
    expect(surface()).toHaveClass("transition-[opacity]", "duration-fast");
  });

  test("unread is bright and semibold, read calms to the second tier, and no bar is drawn", () => {
    const { rerender } = renderRow({ unread: true });
    expect(screen.getByText("Pat Colleague")).toHaveClass("font-semibold", "text-text-1");
    expect(screen.getByText("RustConf invite")).toHaveClass("text-text-1");
    expect(screen.getByTestId("row")).toHaveAttribute("data-unread", "true");
    expect(screen.queryByTestId("unread-bar")).toBeNull();

    rerender(
      <ul>
        <Row
          lit={false}
          unread={false}
          primary="Pat Colleague"
          secondary="RustConf invite"
          data-testid="row"
        />
      </ul>,
    );
    expect(screen.getByText("Pat Colleague")).toHaveClass("text-text-2");
    expect(screen.getByText("Pat Colleague")).not.toHaveClass("font-semibold");
    expect(screen.getByText("RustConf invite")).toHaveClass("text-text-2");
    expect(screen.getByTestId("row")).not.toHaveAttribute("data-unread");
  });

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

  test("a list with no read state keeps the neutral text tier", () => {
    renderRow();
    expect(screen.getByText("Pat Colleague")).toHaveClass("text-text-1");
    expect(screen.getByText("RustConf invite")).toHaveClass("text-text-2");
    expect(screen.getByTestId("row")).not.toHaveAttribute("data-unread");
  });

  test("verbs replace the trail on the lit row and show on pointer hover, never both", () => {
    const { rerender } = renderRow();
    const trail = () => screen.getByText("2h").parentElement!;
    const verbs = () => screen.getByRole("button", { name: "reply" }).parentElement!;
    expect(trail()).not.toHaveClass("hidden");
    expect(verbs()).toHaveClass("hidden", "pointer-fine:group-hover/row:flex");
    rerender(
      <ul>
        <Row
          lit
          primary="Pat"
          trail={<time>2h</time>}
          verbs={<RowVerb keys="r" label="reply" onClick={() => {}} />}
          data-testid="row"
        />
      </ul>,
    );
    expect(trail()).toHaveClass("hidden");
    expect(verbs()).toHaveClass("flex");
    expect(verbs()).not.toHaveClass("hidden");
  });

  test("the lit row keeps its time, after the actions, and the trail's marks go", () => {
    renderRow({
      lit: true,
      trail: <span>marks</span>,
      time: <time data-testid="row-time">Tue 21 Oct, 14:02</time>,
    });
    const time = screen.getByTestId("row-time");
    expect(time.parentElement).not.toHaveClass("hidden");
    expect(screen.getByText("marks").parentElement).toHaveClass("hidden");
    const actions = screen.getByRole("button", { name: "reply" }).parentElement!;
    expect(actions.compareDocumentPosition(time) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  test("at a phone width the actions clip before the time, which is still rendered", () => {
    renderRow({
      lit: true,
      trail: <span>marks</span>,
      time: <time data-testid="row-time">Tue 21 Oct, 14:02</time>,
    });
    const time = screen.getByTestId("row-time");
    // jsdom applies no layout, so the phone rule is asserted as the classes that carry it.
    const group = time.parentElement!.parentElement!;
    expect(group).toHaveClass("max-sm:max-w-[60%]");
    const actions = screen.getByRole("button", { name: "reply" }).parentElement!;
    expect(actions).toHaveClass("min-w-0", "max-sm:overflow-hidden");
    expect(time.parentElement).toHaveClass("shrink-0");
    expect(time).toBeInTheDocument();
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

  test("a leaving row stops taking clicks while it folds", () => {
    renderRow({ leaving: true });
    expect(screen.getByTestId("row")).toHaveClass("row-fold", "pointer-events-none");
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

    test("a change to another row leaves this one alone", () => {
      renderRow({ land: ["thread-1"] });
      act(() => useLanding.getState().land(["thread-2"]));
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

    test("no mark when the view is scoped to one account", () => {
      renderRow({ accountId: "work" }, identitiesFrom(ACCOUNTS, true));
      expect(screen.queryByRole("img")).toBeNull();
    });

    test("no mark for a row whose account is unknown, or for a one-account user", () => {
      const { unmount } = renderRow({ accountId: "nobody" }, identitiesFrom(ACCOUNTS, false));
      expect(screen.queryByRole("img")).toBeNull();
      unmount();
      renderRow({ accountId: "work" }, identitiesFrom(ACCOUNTS.slice(0, 1), false));
      expect(screen.queryByRole("img")).toBeNull();
    });

    test("an older daemon with no identity draws no mark", () => {
      const withoutIdentity = ACCOUNTS.map(({ identity: _identity, ...account }) => account);
      renderRow({ accountId: "work" }, identitiesFrom(withoutIdentity, false));
      expect(screen.queryByRole("img")).toBeNull();
    });

    test("the chip stays on the lit row, in front of the verbs, with the time after", () => {
      renderRow(
        { accountId: "home", lit: true, time: <span>10:02</span> },
        identitiesFrom(ACCOUNTS, false),
      );
      const chip = screen.getByRole("img", { name: "Home" });
      const verb = screen.getByRole("button", { name: "reply" });
      const time = screen.getByText("10:02");
      expect(chip.compareDocumentPosition(verb) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(verb.compareDocumentPosition(time) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(within(screen.getByTestId("row")).getByText("10:02")).toBeVisible();
    });
  });
});
