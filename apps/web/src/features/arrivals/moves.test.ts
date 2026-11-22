import { beforeEach, describe, expect, test, vi } from "vitest";

import { runLatestUndo, useUndo } from "@/state/undoStore";

import { performMove, useSenderAsk } from "./moves";
import { outcome } from "./testing";
import { resetHintsForTests } from "./useTrustHint";

const fetchMock = vi.hoisted(() => vi.fn<(path: string, opts?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));

interface ToastOptions {
  description?: string;
  action?: { label: string };
  cancel?: { label: string };
}

const toastMock = vi.hoisted(() => ({
  success: vi.fn<(message: string, options?: ToastOptions) => void>(),
  error: vi.fn<(...args: unknown[]) => void>(),
  info: vi.fn<(message: string, options?: ToastOptions) => void>(),
  dismiss: vi.fn<(...args: unknown[]) => void>(),
}));
vi.mock("sonner", () => ({ toast: toastMock }));

beforeEach(() => {
  fetchMock.mockReset();
  for (const fn of Object.values(toastMock)) fn.mockReset();
  useUndo.getState().clear();
  useSenderAsk.getState().set(null);
  resetHintsForTests();
});

const moved = (overrides = {}) => ({ kind: "MessageMoved", outcome: outcome(overrides) });

describe("moving an email", () => {
  test("posts the move and offers undo, the sender question and the first hint", async () => {
    fetchMock.mockResolvedValueOnce(moved());
    await performMove({ messageId: "msg-1", mode: "reading" });

    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/messages/msg-1/move", {
      method: "POST",
      body: { mode: "reading", sender: false, dry_run: false },
    });
    const [message, options] = toastMock.info.mock.calls[0]!;
    expect(message).toBe("Moved to Reading. Always for this sender? (P)");
    expect(options?.description).toMatch(/^X moves just this email/);
    expect(options?.action?.label).toBeTruthy();
    expect(options?.cancel?.label).toBe("Always for this sender");
    expect(useSenderAsk.getState().pending?.message_id).toBe("msg-1");
  });

  test("u undoes the newest move through the daemon", async () => {
    fetchMock.mockResolvedValueOnce(moved());
    await performMove({ messageId: "msg-1", mode: "reading" });
    fetchMock.mockResolvedValueOnce({
      kind: "MoveUndone",
      correction_id: 7,
      copy: "Moved back to Updates.",
    });
    expect(runLatestUndo()).toBe("mail");
    await vi.waitFor(() =>
      expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/mail/moves/7/undo", { method: "POST" }),
    );
    await vi.waitFor(() =>
      expect(toastMock.success).toHaveBeenLastCalledWith("Moved back to Updates."),
    );
    expect(useUndo.getState().lastUndo).toBeNull();
    expect(useSenderAsk.getState().pending).toBeNull();
  });
});
