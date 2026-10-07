import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { AckPlan } from "./api";

const previewAck = vi.fn<(threadId: string, signal: AbortSignal) => Promise<AckPlan>>();
const sendAck = vi.fn<(plan: AckPlan) => Promise<AckPlan>>();
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  previewAck: (threadId: string, signal: AbortSignal) => previewAck(threadId, signal),
  sendAck: (plan: AckPlan) => sendAck(plan),
  refreshMessages: async () => {},
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

const { useGotIt } = await import("./useGotIt");

function plan(extra: Partial<AckPlan> = {}): AckPlan {
  return {
    account_id: "acct",
    thread_id: "thread-1",
    reply_to_message_id: "m1",
    from: "alex@demo.mxr.local",
    to: [{ email: "samir@launchpad.example", name: "Samir Patel" }],
    subject: "Re: Contract renewal",
    text: "Thanks Samir, got it.",
    built_from: "No greeting or sign-off of yours with Samir to go on, so a plain thanks.",
    countdown_seconds: 5,
    dry_run: true,
    preview_token: "token-abc",
    preview_expires_at: "2026-10-07T04:01:00Z",
    ...extra,
  };
}

/** A promise the test resolves when it chooses. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

beforeEach(() => {
  vi.useFakeTimers();
  sendAck.mockImplementation(async (sent) => ({ ...sent, dry_run: false }));
});

afterEach(() => {
  vi.useRealTimers();
  previewAck.mockReset();
  sendAck.mockReset();
});

describe("useGotIt", () => {
  test("a send carries the preview's token and exact text", async () => {
    previewAck.mockResolvedValue(plan());
    const { result } = renderHook(({ scope }) => useGotIt(scope), {
      initialProps: { scope: "person:samir|thread-1" },
    });
    await act(async () => {
      await result.current.start("thread-1");
    });
    expect(result.current.pending?.plan.text).toBe("Thanks Samir, got it.");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    expect(sendAck).toHaveBeenCalledTimes(1);
    expect(sendAck.mock.calls[0]![0]).toMatchObject({
      thread_id: "thread-1",
      text: "Thanks Samir, got it.",
      preview_token: "token-abc",
    });
  });

  test("unmounting mid-countdown sends nothing", async () => {
    previewAck.mockResolvedValue(plan());
    const { result, unmount } = renderHook(() => useGotIt("person:samir|thread-1"));
    await act(async () => {
      await result.current.start("thread-1");
    });
    expect(result.current.pending).not.toBeNull();
    unmount();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(sendAck).not.toHaveBeenCalled();
  });

  test("changing person or topic mid-countdown sends nothing", async () => {
    previewAck.mockResolvedValue(plan());
    const { result, rerender } = renderHook(({ scope }) => useGotIt(scope), {
      initialProps: { scope: "person:samir|thread-1" },
    });
    await act(async () => {
      await result.current.start("thread-1");
    });
    rerender({ scope: "person:jon|thread-2" });
    expect(result.current.pending).toBeNull();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(sendAck).not.toHaveBeenCalled();
  });

  test("a preview that resolves after unmount registers nothing", async () => {
    const answer = deferred<AckPlan>();
    let signal: AbortSignal | undefined;
    previewAck.mockImplementation((_thread, abort) => {
      signal = abort;
      return answer.promise;
    });
    const { result, unmount } = renderHook(() => useGotIt("person:samir|thread-1"));
    let started: Promise<void> | undefined;
    act(() => {
      started = result.current.start("thread-1");
    });
    unmount();
    expect(signal?.aborted).toBe(true);
    answer.resolve(plan());
    await started;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(sendAck).not.toHaveBeenCalled();
  });

  test("a preview without a token is refused, not sent", async () => {
    previewAck.mockResolvedValue(plan({ preview_token: null }));
    const { result } = renderHook(() => useGotIt("person:samir|thread-1"));
    await act(async () => {
      await result.current.start("thread-1");
    });
    expect(result.current.pending).toBeNull();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(sendAck).not.toHaveBeenCalled();
  });

  test("undo during the countdown sends nothing", async () => {
    previewAck.mockResolvedValue(plan());
    const { result } = renderHook(() => useGotIt("person:samir|thread-1"));
    await act(async () => {
      await result.current.start("thread-1");
    });
    act(() => result.current.undo());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(sendAck).not.toHaveBeenCalled();
  });
});
