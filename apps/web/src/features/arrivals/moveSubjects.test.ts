import { beforeEach, describe, expect, test, vi } from "vitest";

import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import type { NowItem } from "@/features/now/nowItems";

import { moveCommands, nowMoveSubject } from "./moveSubjects";
import { useSenderAsk } from "./moves";
import { outcome } from "./testing";

vi.mock("@/lib/daemonAvailability", () => ({ refuseWhileDaemonDown: () => false }));
const fetchMock = vi.hoisted(() => vi.fn<(path: string, opts?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(...args: unknown[]) => void>(),
    error: vi.fn<(...args: unknown[]) => void>(),
    info: vi.fn<(...args: unknown[]) => void>(),
    dismiss: vi.fn<(...args: unknown[]) => void>(),
  },
}));

// SAFETY: nowMoveSubject reads only the fields below; the rest of a Now row
// is irrelevant to which email X moves.
const person = {
  kind: "person",
  key: "person:t1",
  threadId: "t1",
  person: {
    why: "",
    row: {
      message_id: "msg-1",
      subject: "Lease",
      counterparty_email: "sam@example.com",
      counterparty_name: "Sam",
    },
  },
} as unknown as NowItem;

beforeEach(() => {
  useMailDialogs.getState().close();
  useSenderAsk.getState().set(null);
  fetchMock.mockReset();
});

describe("X and K on a row", () => {
  test("a person on Now moves the email their row stands for", () => {
    expect(nowMoveSubject(person)).toEqual({
      messageId: "msg-1",
      label: "Lease",
      senderLabel: "Sam",
    });
    // The Updates card is a digest, not one email.
    // SAFETY: an Updates card's contents are never read for a move.
    expect(nowMoveSubject({ kind: "updates", key: "updates" } as unknown as NowItem)).toBeNull();
  });

  test("X opens the email picker and K the sender picker", () => {
    const commands = moveCommands(() => nowMoveSubject(person));
    commands.moveToMode();
    expect(useMailDialogs.getState().dialog).toMatchObject({ kind: "move-to-mode", sender: false });
    commands.moveSenderToMode();
    expect(useMailDialogs.getState().dialog).toMatchObject({ kind: "move-to-mode", sender: true });
  });

  test("K while a move's toast asks answers it instead", () => {
    useSenderAsk.getState().set(outcome());
    fetchMock.mockResolvedValueOnce({ kind: "MessageMoved", outcome: outcome({ sender: true }) });
    moveCommands(() => nowMoveSubject(person)).moveSenderToMode();
    expect(useMailDialogs.getState().dialog).toBeNull();
    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/messages/msg-1/move", {
      method: "POST",
      body: { mode: "reading", sender: true, dry_run: false },
    });
  });
});
