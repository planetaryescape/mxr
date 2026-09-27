import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { UnsubscribePurgeResponse } from "@/features/mailbox/api";
import type { MessageRowView } from "@/features/mailbox/types";
import { useUndo } from "@/state/undoStore";

import { targetFromRows } from "../target";
import { UnsubscribeDialog } from "./UnsubscribeDialog";

type PurgeInput = {
  address: string;
  accountId?: string;
  dryRun?: boolean;
  archiveOnNoMethod?: boolean;
};

const api = vi.hoisted(() => ({
  unsubscribeAndClearSender: vi.fn<(input: PurgeInput) => Promise<UnsubscribePurgeResponse>>(),
  unsubscribeFromSender:
    vi.fn<(input: { messageId: string; archive: boolean }) => Promise<unknown>>(),
}));
vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  ...api,
}));

const toast = vi.hoisted(() => ({
  success: vi.fn<(message: string, options?: { action?: { label: string } }) => void>(),
  error: vi.fn<(message: string, options?: unknown) => void>(),
}));
vi.mock("sonner", () => ({ toast }));

const newsletter: MessageRowView = {
  id: "msg-9",
  kind: "thread",
  account_id: "acc-1",
  thread_id: "thread-9",
  provider_id: "p9",
  sender: "Weekly Digest <news@digest.example>",
  subject: "This week",
  snippet: "",
  date: "2026-09-26T10:00:00Z",
  date_label: "",
  date_full: "",
  date_relative: "",
  unread: true,
  starred: false,
  has_attachments: false,
  message_ids: ["msg-9"],
};

function preview(overrides: Partial<NonNullable<UnsubscribePurgeResponse["result"]>> = {}) {
  return {
    ok: true,
    result: {
      address: "news@digest.example",
      status: "ok",
      method: { OneClick: { url: "https://digest.example/u" } },
      message_count: 12,
      archived_count: 0,
      ...overrides,
    },
  } satisfies UnsubscribePurgeResponse;
}

let onClose: ReturnType<typeof vi.fn<() => void>>;
let onDone: ReturnType<typeof vi.fn<() => void>>;

function renderDialog() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <UnsubscribeDialog
        target={targetFromRows([newsletter], "list")}
        onClose={onClose}
        onDone={onDone}
      />
    </QueryClientProvider>,
  );
}

function press(key: string) {
  fireEvent.keyDown(screen.getByRole("dialog"), { key });
}

beforeEach(() => {
  onClose = vi.fn<() => void>();
  onDone = vi.fn<() => void>();
  useUndo.setState({ lastMutationId: null, lastUndo: null });
  api.unsubscribeFromSender.mockResolvedValue({ ok: true });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("UnsubscribeDialog", () => {
  test("previews how many messages archiving would take, from a dry run", async () => {
    api.unsubscribeAndClearSender.mockResolvedValue(preview());
    renderDialog();

    expect(screen.getByRole("heading", { name: "Unsubscribe from Weekly Digest?" })).toBeVisible();
    expect(await screen.findByText("Unsubscribe and archive 12 messages")).toBeVisible();
    expect(screen.getByText(/One-click unsubscribe/)).toBeVisible();
    expect(api.unsubscribeAndClearSender).toHaveBeenCalledWith({
      address: "news@digest.example",
      accountId: "acc-1",
      dryRun: true,
    });
  });

  test("a archives everything from the sender, with undo", async () => {
    api.unsubscribeAndClearSender
      .mockResolvedValueOnce(preview())
      .mockResolvedValueOnce(preview({ archived_count: 12, mutation_id: "mut-7" }));
    renderDialog();
    await screen.findByText("Unsubscribe and archive 12 messages");

    press("a");

    expect(onClose).toHaveBeenCalled();
    expect(onDone).toHaveBeenCalled();
    expect(api.unsubscribeAndClearSender).toHaveBeenLastCalledWith({
      address: "news@digest.example",
      accountId: "acc-1",
      archiveOnNoMethod: false,
    });
    await vi.waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        "Unsubscribed and archived 12 messages",
        expect.objectContaining({ action: expect.objectContaining({ label: "Undo archive" }) }),
      ),
    );
    expect(useUndo.getState().lastMutationId).toBe("mut-7");
    expect(api.unsubscribeFromSender).not.toHaveBeenCalled();
  });

  test("u unsubscribes and keeps existing mail", async () => {
    api.unsubscribeAndClearSender.mockResolvedValue(preview());
    renderDialog();
    await screen.findByText("Unsubscribe and archive 12 messages");

    press("u");

    expect(api.unsubscribeFromSender).toHaveBeenCalledWith({ messageId: "msg-9", archive: false });
    // Only the dry run ever went to the purge endpoint.
    expect(api.unsubscribeAndClearSender).toHaveBeenCalledTimes(1);
    expect(onDone).not.toHaveBeenCalled();
    await vi.waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Unsubscribed from Weekly Digest"),
    );
  });

  test("a waits for the preview before archiving", async () => {
    api.unsubscribeAndClearSender.mockReturnValue(new Promise(() => undefined));
    renderDialog();

    press("a");

    expect(onClose).not.toHaveBeenCalled();
    expect(api.unsubscribeAndClearSender).toHaveBeenCalledTimes(1);
  });

  test("a sender with no unsubscribe method says archiving is all it does", async () => {
    api.unsubscribeAndClearSender
      .mockResolvedValueOnce(preview({ status: "no_method", method: "None", message_count: 3 }))
      .mockResolvedValueOnce(preview({ status: "no_method", archived_count: 3 }));
    renderDialog();

    expect(await screen.findByText(/No unsubscribe link found/)).toBeVisible();
    press("a");

    expect(api.unsubscribeAndClearSender).toHaveBeenLastCalledWith(
      expect.objectContaining({ archiveOnNoMethod: true }),
    );
  });

  // Enter on a focused option must run that option, not the Enter shortcut.
  test("Enter on the focused archive button archives", async () => {
    api.unsubscribeAndClearSender.mockResolvedValue(preview());
    renderDialog();
    const archive = await screen.findByRole("button", { name: /Unsubscribe and archive 12/ });
    archive.focus();

    // A browser clicks a focused button on Enter unless keydown was cancelled.
    if (fireEvent.keyDown(archive, { key: "Enter" })) fireEvent.click(archive);

    expect(api.unsubscribeFromSender).not.toHaveBeenCalled();
    expect(api.unsubscribeAndClearSender).toHaveBeenCalledTimes(2);
  });
});
