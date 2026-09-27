import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { MessageRowView, MutationResponse } from "@/features/mailbox/types";

import { targetFromRows } from "../target";
import { MoveDialog } from "./MoveDialog";

type RouteInput = {
  messageIds: string[];
  toLabel: string;
  fromQueueLabel: string;
  archive?: boolean;
  dryRun?: boolean;
};

const api = vi.hoisted(() => ({
  routeMessages: vi.fn<(input: RouteInput) => Promise<MutationResponse>>(),
}));
vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  ...api,
}));
vi.mock("@/features/mailbox/useMailboxQuery", () => ({ useShellQuery: () => ({ data: {} }) }));
vi.mock("@/features/mailbox/lenses", () => ({
  lensesFromShell: () => [
    { section: "labels", label: "Work" },
    { section: "labels", label: "Receipts" },
  ],
}));
const mutations = vi.hoisted(() => ({
  performMailAction:
    vi.fn<(action: string, ids: string[], options?: unknown) => Promise<unknown>>(),
}));
vi.mock("../mailMutations", () => mutations);

function row(id: string): MessageRowView {
  return {
    id,
    kind: "message",
    account_id: "acc-1",
    thread_id: `thread-${id}`,
    provider_id: id,
    sender: "Ada <ada@example.com>",
    subject: "Invoice",
    snippet: "",
    date: "2026-09-26T10:00:00Z",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: true,
    starred: false,
    has_attachments: false,
  };
}

function renderRoute(ids: string[]) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MoveDialog
        target={targetFromRows(ids.map(row), "list")}
        route={{ fromQueueLabel: "Work" }}
        onClose={() => {}}
      />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  // cmdk measures its list and scrolls the active item into view.
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView = () => undefined;
  api.routeMessages.mockReset();
  mutations.performMailAction.mockReset();
});

describe("routing a batch out of a queue", () => {
  test("previews with a dry run over the same ids, then routes on confirm", async () => {
    api.routeMessages.mockResolvedValue({
      ok: true,
      result: { requested: 2, succeeded: 0, skipped: 2, failed: 0 },
    });
    renderRoute(["m-1", "m-2"]);

    fireEvent.click(screen.getByRole("option", { name: /Receipts/ }));

    expect(
      await screen.findByText(
        "2 messages will be labelled Receipts, leave Work, be marked read and archived.",
      ),
    ).toBeInTheDocument();
    expect(api.routeMessages).toHaveBeenCalledWith({
      messageIds: ["m-1", "m-2"],
      toLabel: "Receipts",
      fromQueueLabel: "Work",
      archive: true,
      dryRun: true,
    });
    expect(mutations.performMailAction).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /Route 2 messages/ }));
    expect(mutations.performMailAction).toHaveBeenCalledWith("route", ["m-1", "m-2"], {
      payload: { label: "Receipts", fromQueueLabel: "Work", archive: true },
    });
  });

  test("a single message routes straight away", () => {
    renderRoute(["m-1"]);
    fireEvent.click(screen.getByRole("option", { name: /Receipts/ }));
    expect(api.routeMessages).not.toHaveBeenCalled();
    expect(mutations.performMailAction).toHaveBeenCalledWith("route", ["m-1"], {
      payload: { label: "Receipts", fromQueueLabel: "Work", archive: true },
    });
  });
});
