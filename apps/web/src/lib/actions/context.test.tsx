/* @vitest-environment jsdom */

import { renderHook } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { useActionContext } from "./context";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useSelection } from "@/state/selectionStore";

const router = vi.hoisted(() => ({ pathname: "/m/inbox" }));

vi.mock("@tanstack/react-router", () => ({
  useRouterState: ({
    select,
  }: {
    select: (state: { location: { pathname: string } }) => unknown;
  }) => select({ location: { pathname: router.pathname } }),
}));

beforeEach(() => {
  router.pathname = "/m/inbox";
  useSelection.setState({ ids: new Set(), lastClickedId: null, scope: null });
  useMailboxPane.setState({
    activePane: "mailbox",
    suppressNextReaderFocus: false,
    sidebarIndex: 0,
  });
});

describe("useActionContext", () => {
  test("recomputes when selection size changes", () => {

    const { result } = renderHook(() => useActionContext({ accountCount: 1 }));
    expect(result.current.selectionCount).toBe(0);

    act(() => {
      useSelection.setState({
        ids: new Set(["m1", "m2"]),
        lastClickedId: "m2",
        scope: "/m/inbox",
      });
    });

    expect(result.current.selectionCount).toBe(2);
  });

  test("marks hasFocusedThread when path matches /m/:lens/:id", () => {
    router.pathname = "/m/inbox";
    const { result: noThread } = renderHook(() => useActionContext({ accountCount: 1 }));
    expect(noThread.current.hasFocusedThread).toBe(false);

    router.pathname = "/m/inbox/thread-abc";
    const { result: withThread } = renderHook(() => useActionContext({ accountCount: 1 }));
    expect(withThread.current.hasFocusedThread).toBe(true);
  });
});
