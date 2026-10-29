/* @vitest-environment jsdom */

import { renderHook } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import type { ReaderNav } from "@/features/mailbox/readerNav";

import { useThreadSiblings } from "./useThreadSiblings";

function navOf(open = vi.fn<ReaderNav["open"]>()): ReaderNav {
  return {
    threads: () => [
      { thread_id: "shared", account_id: "A" },
      { thread_id: "shared", account_id: "B" },
      { thread_id: "other", account_id: "B" },
    ],
    open,
    close: vi.fn<() => void>(),
  };
}

describe("stepping through a list with a thread id held by two accounts", () => {
  test("A's copy is first and B's second, so n from A opens B's, not A's again", () => {
    const open = vi.fn<ReaderNav["open"]>();
    const { result } = renderHook(() => useThreadSiblings("shared", navOf(open), "A"));
    expect(result.current.position).toEqual({ index: 0, total: 3 });
    result.current.step(1);
    expect(open).toHaveBeenCalledWith("shared", {
      focusReader: true,
      accountId: "B",
    });
  });

  test("B's copy knows its own place", () => {
    const { result } = renderHook(() => useThreadSiblings("shared", navOf(), "B"));
    expect(result.current.position).toEqual({ index: 1, total: 3 });
  });

  test("leaving B's copy opens the next row with its account", () => {
    const open = vi.fn<ReaderNav["open"]>();
    const { result } = renderHook(() => useThreadSiblings("shared", navOf(open), "B"));
    result.current.leave(1);
    expect(open).toHaveBeenCalledWith("other", {
      focusReader: true,
      accountId: "B",
    });
  });
});
