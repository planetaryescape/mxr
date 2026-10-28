import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { getRegistry } from "@/lib/actions";
import { snapshotActionContext } from "@/lib/actions/context";
import { setController } from "@/lib/keys/controllers";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useKeyScope } from "@/state/keyScopeStore";
import { SHORTCUT_GUIDE_DELAY_MS, ShortcutGuide } from "./ShortcutGuide";

vi.mock("@tanstack/react-router", async (original) => ({
  ...(await original<typeof import("@tanstack/react-router")>()),
  useRouterState: () => "/messages",
}));

let dispose: () => void;
beforeEach(() => {
  vi.useFakeTimers();
  useKeyScope.setState({ stack: ["messages"], pendingPrefix: null });
  setController("messages", { filter: vi.fn<() => void>() });
  dispose = installKeyDispatcher(window, {
    registry: getRegistry(),
    context: snapshotActionContext,
    onPendingChange: (prefix) => useKeyScope.getState().setPendingPrefix(prefix),
  });
});
afterEach(() => {
  cleanup();
  dispose();
  setController("messages", null);
  useKeyScope.setState({ stack: [], pendingPrefix: null });
  vi.useRealTimers();
});

test("a paused prefix shows live next keys without moving focus, and Escape cancels it", () => {
  render(
    <>
      <button>Current focus</button>
      <ShortcutGuide />
    </>,
  );
  const button = screen.getByRole("button");
  button.focus();
  fireEvent.keyDown(button, { key: "g" });
  act(() => {
    vi.advanceTimersByTime(SHORTCUT_GUIDE_DELAY_MS - 1);
  });
  expect(screen.queryByRole("complementary", { name: "Next shortcut keys" })).toBeNull();
  act(() => {
    vi.advanceTimersByTime(1);
  });
  expect(screen.getByRole("complementary", { name: "Next shortcut keys" })).toHaveTextContent(
    "Go to Inbox",
  );
  expect(screen.getByText("Filter")).toBeInTheDocument();
  expect(button).toHaveFocus();
  fireEvent.keyDown(button, { key: "Escape" });
  expect(screen.queryByRole("complementary")).toBeNull();
  expect(useKeyScope.getState().pendingPrefix).toBeNull();
});

test("fast chords never flash a guide, and typing never starts one", () => {
  render(
    <>
      <input aria-label="Query" />
      <ShortcutGuide />
    </>,
  );
  fireEvent.keyDown(document.body, { key: "g" });
  fireEvent.keyDown(document.body, { key: "f" });
  act(() => {
    vi.advanceTimersByTime(300);
  });
  expect(screen.queryByRole("complementary")).toBeNull();
  const input = screen.getByRole("textbox");
  input.focus();
  fireEvent.keyDown(input, { key: "g" });
  act(() => {
    vi.advanceTimersByTime(300);
  });
  expect(screen.queryByRole("complementary")).toBeNull();
});
