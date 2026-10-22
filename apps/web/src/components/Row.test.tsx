/* @vitest-environment jsdom */

import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { LANDING_MS, useLanding } from "@/state/landingStore";

import { Row, RowVerb } from "./Row";

function renderRow(props: Partial<React.ComponentProps<typeof Row>> = {}) {
  return render(
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
    </ul>,
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

  test("lit draws the lamp surface and the hairline; unlit draws neither", () => {
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
    expect(layers().filter((layer) => layer.classList.contains("opacity-100"))).toHaveLength(2);
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

  test("the unread bar shows on an unlit unread row and hides while lit", () => {
    const { rerender } = renderRow({ unread: true });
    expect(screen.getByTestId("unread-bar")).toHaveClass("w-[2px]", "bg-text-1");
    rerender(
      <ul>
        <Row lit unread primary="Pat" data-testid="row" />
      </ul>,
    );
    expect(screen.queryByTestId("unread-bar")).toBeNull();
    // Weight still says unread, so it stays on the row for the primary line to use.
    expect(screen.getByTestId("row")).toHaveAttribute("data-unread", "true");
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
});
