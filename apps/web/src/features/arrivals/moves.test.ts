import { beforeEach, describe, expect, test, vi } from "vitest";

import { runLatestUndo, useUndo } from "@/state/undoStore";

import { answerPendingSenderAsk, performMove, useSenderAsk } from "./moves";
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
  info: vi.fn<(...args: unknown[]) => void>(),
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
    expect(message).toBe("Moved to Reading. Always for this sender? (K)");
    expect(options?.description).toMatch(/^X moves just this email/);
    expect(options?.action?.label).toBe("Undo");
    expect(options?.cancel?.label).toBe("Always for this sender");
    expect(useSenderAsk.getState().pending?.message_id).toBe("msg-1");
  });

  test("shows the hint on the first move's toast only", async () => {
    fetchMock.mockResolvedValue(moved());
    await performMove({ messageId: "msg-1", mode: "reading" });
    await performMove({ messageId: "msg-1", mode: "reading" });
    expect(toastMock.info.mock.calls[1]![1]?.description).toBe("Press u to undo");
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
    // Undone: the slot is empty, so a second u reaches nothing older.
    expect(useUndo.getState().lastUndo).toBeNull();
    expect(useSenderAsk.getState().pending).toBeNull();
  });

  test("K while the toast asks sends the sender's mail there instead of opening a picker", async () => {
    fetchMock.mockResolvedValueOnce(moved());
    await performMove({ messageId: "msg-1", mode: "reading" });
    fetchMock.mockResolvedValueOnce(
      moved({
        sender: true,
        ask_sender: null,
        hint: null,
        copy: "All mail from maya@orbit.example goes to Reading.",
      }),
    );
    expect(answerPendingSenderAsk()).toBe(true);
    await vi.waitFor(() =>
      expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/mail/messages/msg-1/move", {
        method: "POST",
        body: { mode: "reading", sender: true, dry_run: false },
      }),
    );
    expect(answerPendingSenderAsk()).toBe(false);
  });

  test("a move that changed nothing has no undo", async () => {
    fetchMock.mockResolvedValueOnce(
      moved({ copy: "Already in Reading.", correction_id: null, ask_sender: null, hint: null }),
    );
    await performMove({ messageId: "msg-1", mode: "reading" });
    expect(toastMock.info).toHaveBeenCalledWith("Already in Reading.", { id: "move-msg-1" });
    expect(useUndo.getState().lastUndo).toBeNull();
  });

  test("a failure says so and leaves no undo behind", async () => {
    fetchMock.mockRejectedValueOnce(new Error("No message msg-1."));
    expect(await performMove({ messageId: "msg-1", mode: "reading" })).toBeNull();
    expect(toastMock.error).toHaveBeenCalledWith("Couldn't move it", {
      description: "No message msg-1.",
    });
    expect(useUndo.getState().lastUndo).toBeNull();
  });
});
