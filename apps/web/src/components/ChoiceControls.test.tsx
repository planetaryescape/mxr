import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { expect, test, vi } from "vitest";

import { Page, PageTabs } from "./Page";
import { Segmented } from "./Segmented";
import { runCommand } from "@/lib/keys/controllers";
import { focusActivePane } from "@/lib/keys/focusPane";

function Choices({ tabs = false }: { tabs?: boolean }) {
  const [value, setValue] = useState("one");
  return tabs ? (
    <PageTabs
      label="Views"
      value={value}
      tabs={[
        { id: "one", label: "One" },
        { id: "two", label: "Two" },
        { id: "three", label: "Three" },
      ]}
      onChange={setValue}
    />
  ) : (
    <Segmented
      label="Views"
      value={value}
      options={[
        ["one", "One"],
        ["two", "Two"],
        ["three", "Three"],
      ]}
      onChange={setValue}
    />
  );
}

test.each([false, true])(
  "choice controls support arrows, wrap, Home and End (tabs: %s)",
  (tabs) => {
    render(<Choices tabs={tabs} />);
    const role = tabs ? "tab" : "radio";
    const selected = tabs ? "aria-selected" : "aria-checked";
    const one = screen.getByRole(role, { name: "One" });
    const two = screen.getByRole(role, { name: "Two" });
    const three = screen.getByRole(role, { name: "Three" });
    expect(one.tabIndex).toBe(0);
    expect(two.tabIndex).toBe(-1);
    one.focus();
    fireEvent.keyDown(one, { key: "ArrowLeft" });
    expect(three).toHaveFocus();
    expect(three).toHaveAttribute(selected, "true");
    fireEvent.keyDown(three, { key: "ArrowDown" });
    expect(one).toHaveFocus();
    fireEvent.keyDown(one, { key: "End" });
    expect(three).toHaveFocus();
    fireEvent.keyDown(three, { key: "Home" });
    expect(one).toHaveFocus();
    fireEvent.keyDown(one, { key: "ArrowRight" });
    expect(two).toHaveFocus();
    expect(two).toHaveAttribute(selected, "true");
    expect(one.tabIndex).toBe(-1);
    fireEvent.keyDown(two, { key: "ArrowUp" });
    expect(one).toHaveFocus();
  },
);

test("modified arrows and ordinary letters don't change a choice", () => {
  const onChange = vi.fn<(value: string) => void>();
  render(
    <Segmented
      label="View"
      value="one"
      options={[
        ["one", "One"],
        ["two", "Two"],
      ]}
      onChange={onChange}
    />,
  );
  const one = screen.getByRole("radio", { name: "One" });
  fireEvent.keyDown(one, { key: "ArrowRight", metaKey: true });
  fireEvent.keyDown(one, { key: "f" });
  expect(onChange).not.toHaveBeenCalled();
});

test("direct shortcuts are visible beside their choices", () => {
  render(
    <Segmented
      label="View"
      value="one"
      options={[["one", "One"]]}
      keys={{ one: "g t" }}
      onChange={() => {}}
    />,
  );
  expect(screen.getByRole("radio", { name: "One" })).toHaveTextContent("g t");
});

test("tool pages focus their selected tab directly and can recover focus after a dialog", () => {
  render(
    <Page title="Tool" tabs={<Choices tabs />}>
      <p>Results</p>
    </Page>,
  );
  runCommand("page", "controls");
  expect(screen.getByRole("tab", { name: "One" })).toHaveFocus();
  focusActivePane();
  expect(document.activeElement).toHaveAttribute("data-page-pane");
});

test("link-based section headers support arrows without losing native link activation", () => {
  render(
    <Page
      title="Settings"
      tabs={
        <nav>
          <a href="#one" aria-current="page">
            One
          </a>
          <a href="#two">Two</a>
        </nav>
      }
    >
      Settings
    </Page>,
  );
  const first = screen.getByRole("link", { name: "One" });
  const second = screen.getByRole("link", { name: "Two" });
  runCommand("page", "controls");
  expect(first).toHaveFocus();
  fireEvent.keyDown(first, { key: "ArrowRight" });
  expect(second).toHaveFocus();
  expect(second).toHaveAttribute("href", "#two");
  fireEvent.keyDown(second, { key: "Home" });
  expect(first).toHaveFocus();
});

test("filter chips are reachable even when a tool has no header tabs", () => {
  render(
    <Page title="Activity" toolbar={<button>Refresh</button>}>
      <button aria-pressed="true">This week</button>
      <input aria-label="Search activity" />
    </Page>,
  );
  runCommand("page", "controls");
  expect(screen.getByRole("button", { name: "This week" })).toHaveFocus();
});
