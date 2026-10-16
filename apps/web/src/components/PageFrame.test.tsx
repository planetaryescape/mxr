/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { FramePanesContext, PageFrame } from "./PageFrame";
import { usePageTitleStore } from "@/state/pageTitleStore";

afterEach(() => usePageTitleStore.setState({ title: null }));

describe("PageFrame", () => {
  test("one h1 holds the title, in the title role from Title up to Display", () => {
    render(<PageFrame title="Messages" purpose="People you talk with." />);
    const title = screen.getByRole("heading", { level: 1, name: "Messages" });
    expect(title).toHaveClass("type-title", "lg:type-display", "truncate");
    expect(screen.getByTestId("mode-header")).toHaveTextContent("People you talk with.");
  });

  test("the frame's height is fixed: 88px, or 120px with tabs, shorter on phones", () => {
    const { rerender } = render(<PageFrame title="Now" />);
    const column = () => screen.getByTestId("page-frame").firstElementChild;
    expect(column()).toHaveClass("h-16", "sm:h-[88px]");
    rerender(<PageFrame title="Now" tabs={<div role="tablist" />} />);
    expect(column()).toHaveClass("h-24", "sm:h-[120px]");
    expect(screen.getByTestId("page-frame")).toHaveAttribute("data-tabs", "true");
  });

  test("a detail pane may wrap its title to two lines and grow, never to one line cut off", () => {
    render(<PageFrame wrapTitle title="Re: a very long subject" />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveClass("line-clamp-2");
    expect(screen.getByRole("heading", { level: 1 })).not.toHaveClass("truncate");
    expect(screen.getByTestId("page-frame").firstElementChild).toHaveClass("min-h-16");
    expect(screen.getByTestId("page-frame").firstElementChild).not.toHaveClass("h-16");
  });

  test("the title can be rendered with links while its plain text names the page", () => {
    render(<PageFrame title="Desk" titleContent={<a href="/x">3 replies</a>} />);
    expect(screen.getByRole("heading", { level: 1 })).toContainElement(
      screen.getByRole("link", { name: "3 replies" }),
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveAttribute("title", "Desk");
  });

  test("slots that are not given draw nothing: no actions cluster, no menu", () => {
    render(<PageFrame title="Archive" />);
    expect(screen.queryByRole("button")).toBeNull();
  });

  test("the primary action and the ⋯ menu share the title row", async () => {
    const onSelect = vi.fn<() => void>();
    render(
      <PageFrame
        title="Rules"
        primary={<button type="button">New rule</button>}
        overflow={[
          { label: "Export", onSelect },
          { label: "Delete all", onSelect: vi.fn<() => void>(), destructive: true, disabled: true },
        ]}
      />,
    );
    expect(screen.getByRole("button", { name: "New rule" })).toBeInTheDocument();
    // Radix opens its trigger on Enter, which jsdom can send without pointer capture.
    fireEvent.keyDown(screen.getByRole("button", { name: "More actions" }), { key: "Enter" });
    const items = await screen.findAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual(["Export", "Delete all"]);
    expect(items[1]).toHaveAttribute("aria-disabled", "true");
    expect(items[1]).toHaveClass("text-danger");
    fireEvent.click(items[0]!);
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  test("an empty overflow list shows no menu", () => {
    render(<PageFrame title="Rules" overflow={[]} />);
    expect(screen.queryByRole("button", { name: "More actions" })).toBeNull();
  });

  test("freshness sits with the purpose line, or with the tabs when there are tabs", () => {
    const { rerender } = render(<PageFrame title="Archive" purpose="p" freshness="2 records" />);
    expect(screen.getByTestId("mode-header").parentElement).toContainElement(
      screen.getByText("2 records"),
    );
    rerender(
      <PageFrame title="Archive" purpose="p" tabs={<span>tabs</span>} freshness="2 records" />,
    );
    expect(screen.getByTestId("mode-header").parentElement).not.toContainElement(
      screen.getByText("2 records"),
    );
  });

  test("publishes its title to the top bar, unless it is a detail pane", () => {
    const { rerender } = render(<PageFrame title="Todo" />);
    expect(usePageTitleStore.getState().title).toBe("Todo");
    rerender(<PageFrame title="Pat Colleague" publishTitle={false} />);
    expect(usePageTitleStore.getState().title).toBeNull();
  });

  test("a pane frame beside the page's frame leaves the page's title published", () => {
    render(
      <>
        <PageFrame title="Messages" />
        <PageFrame title="Pat Colleague" publishTitle={false} />
      </>,
    );
    expect(usePageTitleStore.getState().title).toBe("Messages");
  });

  test("beside a detail pane the list steps down to Title and reserves the detail's toolbar", () => {
    render(
      <FramePanesContext.Provider value={{ split: true, inset: 40 }}>
        <PageFrame title="Inbox" />
        <PageFrame title="Subject" detail wrapTitle />
      </FramePanesContext.Provider>,
    );
    const [list, detail] = screen.getAllByRole("heading", { level: 1 });
    expect(list).not.toHaveClass("lg:type-display");
    expect(detail).toHaveClass("lg:type-display");
    const [listFrame, detailFrame] = screen.getAllByTestId("page-frame");
    expect(listFrame?.firstElementChild).toHaveStyle({ height: "128px", paddingTop: "50px" });
    // The detail stays on one line so its rule stays level with the list's.
    expect(detail).toHaveClass("truncate");
    expect(detailFrame?.firstElementChild).toHaveClass("sm:h-[88px]");
  });

  test("with no split a lone list keeps Display and its plain height", () => {
    render(<PageFrame title="Inbox" />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveClass("lg:type-display");
    expect(screen.getByTestId("page-frame").firstElementChild).not.toHaveAttribute("style");
  });
});
