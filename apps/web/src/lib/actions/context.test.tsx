/* @vitest-environment jsdom */

import { renderHook } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { buildActionContext, useActionContext } from "./context";
import { useKeyScope } from "@/state/keyScopeStore";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useOpenThread } from "@/state/openThreadStore";
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
  useOpenThread.setState({ threadId: null });
  useKeyScope.setState({ stack: [], pendingPrefix: null });
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

  test("hasFocusedThread follows the open reader, not the URL", () => {
    router.pathname = "/m/inbox/thread-abc";
    const { result } = renderHook(() => useActionContext({ accountCount: 1 }));
    expect(result.current.hasFocusedThread).toBe(false);

    act(() => useOpenThread.getState().setThreadId("thread-abc"));
    expect(result.current.hasFocusedThread).toBe(true);

    act(() => useOpenThread.getState().setThreadId(null));
    expect(result.current.hasFocusedThread).toBe(false);
  });

  test("scopes run innermost first and always end in global", () => {
    const { result } = renderHook(() => useActionContext({ accountCount: 1 }));
    expect(result.current.scopes).toEqual(["global"]);

    act(() => {
      useKeyScope.getState().pushScope("list");
      useKeyScope.getState().pushScope("reader");
    });
    expect(result.current.scopes).toEqual(["reader", "list", "global"]);

    act(() => useKeyScope.getState().popScope("reader"));
    expect(result.current.scopes).toEqual(["list", "global"]);
  });
});

describe("buildActionContext", () => {
  test("dedupes repeated scopes and ignores a pushed global", () => {
    const context = buildActionContext({
      path: "/m/inbox",
      activePane: "mailbox",
      scopeStack: ["global", "list", "reader", "list"],
      selectionCount: 0,
      accountCount: 2,
      openThreadId: null,
    });

    expect(context.scopes).toEqual(["list", "reader", "global"]);
    expect(context.isFirstAccountOnly).toBe(false);
  });
});
