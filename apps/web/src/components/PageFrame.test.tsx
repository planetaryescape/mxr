/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { PageFrame } from "./PageFrame";
import { usePageTitleStore } from "@/state/pageTitleStore";

afterEach(() => usePageTitleStore.setState({ title: null }));

describe("PageFrame", () => {
  test("the title can be rendered with links while its plain text names the page", () => {
    render(<PageFrame title="Desk" titleContent={<a href="/x">3 replies</a>} />);
    expect(screen.getByRole("heading", { level: 1 })).toContainElement(
      screen.getByRole("link", { name: "3 replies" }),
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveAttribute("title", "Desk");
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

});
