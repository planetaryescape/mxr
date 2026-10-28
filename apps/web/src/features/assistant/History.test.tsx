/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { AssistantSummary } from "./api";
import { History } from "./History";
import { useKeyScope } from "@/state/keyScopeStore";

const apiFetch = vi.hoisted(() => vi.fn<(path: string, init?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch,
}));

function summary(id: string, title: string): AssistantSummary {
  return {
    id,
    title,
    status: "idle",
    created_at: 0,
    updated_at: 0,
    message_count: 4,
    pending_count: 0,
  };
}

/** The delete calls made, as their dry_run flags. */
const deletes = () =>
  apiFetch.mock.calls
    .filter(([path]) => path.endsWith("/delete"))
    .map(([path, init]) => ({
      path,
      dryRun: (init as { body: { dry_run: boolean } }).body.dry_run,
    }));

function renderHistory() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <History currentId={null} onPick={() => {}} onClose={() => {}} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  apiFetch.mockReset();
  apiFetch.mockImplementation(async (path, init) => {
    if (path.includes("/conversations?")) {
      return { conversations: [summary("c1", "Receipts"), summary("c2", "Old trip")] };
    }
    if (path.endsWith("/delete")) {
      const dryRun = (init as { body: { dry_run: boolean } }).body.dry_run;
      const id = path.includes("/c2/") ? "c2" : "c1";
      return {
        kind: "AssistantDelete",
        delete: {
          conversation_id: id,
          title: id === "c2" ? "Old trip" : "Receipts",
          message_count: 4,
          dry_run: dryRun,
          deleted: !dryRun,
        },
      };
    }
    throw new Error(`Unexpected request: ${path}`);
  });
});

describe("History", () => {
  test("# asks first, with the daemon's preview, and y deletes; d only says so", async () => {
    renderHistory();
    await screen.findByText("Old trip");
    const history = screen.getByRole("heading", { name: "Past conversations" }).parentElement!;

    fireEvent.keyDown(history, { key: "j" });
    // d deleted until K3 (blueprint 35): it names # and asks nothing.
    fireEvent.keyDown(history, { key: "d" });
    expect(useKeyScope.getState().moved).toEqual({ label: "Delete the conversation", key: "#" });
    fireEvent.keyDown(history, { key: "D" });
    expect(useKeyScope.getState().moved).toEqual({ label: "Delete the conversation", key: "#" });
    expect(deletes()).toEqual([]);
    fireEvent.keyDown(history, { key: "#" });

    expect(
      await screen.findByText('Delete "Old trip" and its 4 messages? This can\'t be undone.'),
    ).toBeVisible();
    expect(deletes()).toEqual([
      { path: "/api/v1/mail/assistant/conversations/c2/delete", dryRun: true },
    ]);

    fireEvent.keyDown(history, { key: "y" });

    await waitFor(() =>
      expect(deletes()).toEqual([
        { path: "/api/v1/mail/assistant/conversations/c2/delete", dryRun: true },
        { path: "/api/v1/mail/assistant/conversations/c2/delete", dryRun: false },
      ]),
    );
  });

  test("n, Esc and a bare Enter keep the conversation; y deletes it", async () => {
    const onClose = vi.fn<() => void>();
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <History currentId={null} onPick={() => {}} onClose={onClose} />
      </QueryClientProvider>,
    );
    await screen.findByText("Receipts");
    const history = screen.getByRole("heading", { name: "Past conversations" }).parentElement!;

    fireEvent.keyDown(history, { key: "#" });
    await screen.findByText(/^Delete "Receipts"/);
    fireEvent.keyDown(history, { key: "n" });
    expect(screen.queryByText(/^Delete "Receipts"/)).not.toBeInTheDocument();

    fireEvent.keyDown(history, { key: "#" });
    await screen.findByText(/^Delete "Receipts"/);
    fireEvent.keyDown(history, { key: "Escape" });
    expect(screen.queryByText(/^Delete "Receipts"/)).not.toBeInTheDocument();
    // Esc backed out of the question, not out of the history.
    expect(onClose).not.toHaveBeenCalled();
    expect(deletes().every((call) => call.dryRun)).toBe(true);

    fireEvent.keyDown(history, { key: "#" });
    await screen.findByText(/^Delete "Receipts"/);
    fireEvent.keyDown(history, { key: "Enter" });
    expect(screen.queryByText(/^Delete "Receipts"/)).not.toBeInTheDocument();
    expect(deletes().every((call) => call.dryRun)).toBe(true);

    fireEvent.keyDown(history, { key: "#" });
    await screen.findByText(/^Delete "Receipts"/);
    fireEvent.keyDown(history, { key: "y" });
    await waitFor(() => expect(deletes().at(-1)).toMatchObject({ dryRun: false }));
  });

  test("the arrows move as j and k do, Enter opens, and q closes as Esc does", async () => {
    const onPick = vi.fn<(id: string | null) => void>();
    const onClose = vi.fn<() => void>();
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <History currentId={null} onPick={onPick} onClose={onClose} />
      </QueryClientProvider>,
    );
    await screen.findByText("Old trip");
    const history = screen.getByRole("heading", { name: "Past conversations" }).parentElement!;

    fireEvent.keyDown(history, { key: "ArrowDown" });
    fireEvent.keyDown(history, { key: "ArrowUp" });
    fireEvent.keyDown(history, { key: "ArrowDown" });
    fireEvent.keyDown(history, { key: "Enter" });
    expect(onPick).toHaveBeenCalledWith("c2");

    onClose.mockClear();
    fireEvent.keyDown(history, { key: "q" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
