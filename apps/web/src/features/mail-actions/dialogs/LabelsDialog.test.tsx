import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { MessageLabelView, MessageRowView, ShellResponse } from "@/features/mailbox/types";

import { targetFromRows } from "../target";
import { LabelsDialog } from "./LabelsDialog";

const mailboxApi = vi.hoisted(() => ({
  fetchShell: vi.fn<() => Promise<ShellResponse>>(),
  createLabel: vi.fn<(input: { name: string }) => Promise<unknown>>(),
}));
vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  ...mailboxApi,
}));

const mutations = vi.hoisted(() => ({
  performMailAction:
    vi.fn<(action: string, ids: string[], options?: unknown) => Promise<unknown>>(),
}));
vi.mock("../mailMutations", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../mailMutations")>()),
  performMailAction: mutations.performMailAction,
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(message: string) => void>(),
    error: vi.fn<(message: string) => void>(),
  },
}));

function label(name: string): MessageLabelView {
  return { id: `id-${name}`, name, kind: "user" };
}

function row(id: string, labels: MessageLabelView[]): MessageRowView {
  return {
    id,
    kind: "thread",
    thread_id: `thread-${id}`,
    provider_id: id,
    sender: "Ada",
    subject: `Subject ${id}`,
    snippet: "",
    date: "2026-09-26T10:00:00Z",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
    labels,
    message_ids: [`${id}-1`, `${id}-2`],
  };
}

const shell: ShellResponse = {
  sidebar: {
    sections: [
      {
        id: "labels",
        title: "Labels",
        items: ["Work", "Receipts", "Later"].map((name) => ({
          id: name.toLowerCase(),
          label: name,
          lens: { kind: "label", labelId: `Label_${name}` },
        })),
      },
    ],
  },
};

const target = targetFromRows(
  [row("a", [label("Work"), label("Receipts")]), row("b", [label("Work")])],
  "list",
);

let onClose: ReturnType<typeof vi.fn<() => void>>;

function renderDialog() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <LabelsDialog target={target} onClose={onClose} />
    </QueryClientProvider>,
  );
}

async function presence(name: string) {
  const option = await screen.findByRole("option", { name: new RegExp(`^${name}`) });
  if (option.querySelector(".lucide-check")) return "all";
  if (option.querySelector(".lucide-minus")) return "some";
  return "none";
}

async function toggle(name: string) {
  fireEvent.click(await screen.findByRole("option", { name: new RegExp(`^${name}`) }));
}

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView = () => undefined;
  onClose = vi.fn<() => void>();
  mailboxApi.fetchShell.mockResolvedValue(shell);
  mutations.performMailAction.mockResolvedValue({ ok: true });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe("LabelsDialog", () => {
  test("stages adds and removals and applies them in one change", async () => {
    renderDialog();

    await toggle("Work");
    await toggle("Later");
    await toggle("Receipts");

    expect(screen.getByText("2 adds, 1 removal")).toBeInTheDocument();
    expect(await presence("Work")).toBe("none");
    expect(await presence("Receipts")).toBe("all");

    fireEvent.click(screen.getByRole("button", { name: /apply/i }));

    expect(onClose).toHaveBeenCalled();
    expect(mutations.performMailAction).toHaveBeenCalledWith(
      "labels",
      ["a-1", "a-2", "b-1", "b-2"],
      { payload: { add: ["Later", "Receipts"], remove: ["Work"] } },
    );
  });
});
