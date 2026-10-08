import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { toast } from "sonner";

import type { commitUnsubscribePreview, UnsubscribePurgeResponse } from "@/features/mailbox/api";

import type { ReadingSource } from "./api";
import { ReadingUnsubscribeDialog } from "./ReadingDialogs";

const api = vi.hoisted(() => ({
  commitUnsubscribePreview: vi.fn<typeof commitUnsubscribePreview>(),
  unsubscribeAndClearSender: vi.fn<(input: unknown) => Promise<UnsubscribePurgeResponse>>(),
  unsubscribeFromSender: vi.fn<(input: unknown) => Promise<unknown>>(),
}));
vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  ...api,
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn<(message: string) => void>(), error: vi.fn<(message: string) => void>() },
}));

const source = {
  account_id: "acc-1",
  sender_email: "news@digest.example",
  name: "Weekly Digest",
  evidence: "You opened 0 of the last 11 issues",
} as ReadingSource;

const preview: UnsubscribePurgeResponse = {
  ok: true,
  result: {
    address: "news@digest.example",
    status: "preview",
    method: { OneClick: { url: "https://digest.example/u" } },
    message_count: 12,
    archived_count: 0,
    preview_token: "tok-1",
  },
};

function renderDialog(onClose = vi.fn<() => void>()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <ReadingUnsubscribeDialog source={source} onClose={onClose} />
    </QueryClientProvider>,
  );
  return onClose;
}

beforeEach(() => {
  api.unsubscribeAndClearSender.mockReset().mockResolvedValue(preview);
  api.commitUnsubscribePreview.mockReset().mockResolvedValue(preview);
  api.unsubscribeFromSender.mockReset().mockResolvedValue({});
  vi.mocked(toast.success).mockClear();
  vi.mocked(toast.error).mockClear();
});

describe("ReadingUnsubscribeDialog", () => {
  test("offers just unsubscribe and unsubscribe-and-clear with the previewed count", async () => {
    renderDialog();
    await vi.waitFor(() =>
      expect(screen.getByTestId("unsubscribe-clear")).toHaveTextContent(
        "Unsubscribe and clear 12 issues",
      ),
    );
    expect(screen.getByTestId("unsubscribe-just")).toHaveTextContent(
      "Just unsubscribe — keep what you have",
    );
  });

  test("just unsubscribe commits the previewed sender and method while keeping mail", async () => {
    renderDialog();
    const just = screen.getByTestId("unsubscribe-just");
    await vi.waitFor(() => expect(just).toBeEnabled());
    fireEvent.click(just);
    await vi.waitFor(() =>
      expect(api.commitUnsubscribePreview).toHaveBeenLastCalledWith({
        address: "news@digest.example",
        accountId: "acc-1",
        previewToken: "tok-1",
        archive: false,
      }),
    );
    expect(api.unsubscribeFromSender).not.toHaveBeenCalled();
  });

  test("a commits exactly the previewed token and clears the issues", async () => {
    renderDialog();
    await screen.findByText("Unsubscribe and clear 12 issues");
    await vi.waitFor(() => expect(screen.getByTestId("unsubscribe-clear")).toBeEnabled());
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "a" });
    await vi.waitFor(() =>
      expect(api.commitUnsubscribePreview).toHaveBeenLastCalledWith({
        address: "news@digest.example",
        accountId: "acc-1",
        previewToken: "tok-1",
        archive: true,
      }),
    );
    expect(api.unsubscribeFromSender).not.toHaveBeenCalled();
  });

  test("u just unsubscribes from the keyboard", async () => {
    renderDialog();
    await vi.waitFor(() => expect(screen.getByTestId("unsubscribe-just")).toBeEnabled());
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "u" });
    await vi.waitFor(() =>
      expect(api.commitUnsubscribePreview).toHaveBeenLastCalledWith({
        address: "news@digest.example",
        accountId: "acc-1",
        previewToken: "tok-1",
        archive: false,
      }),
    );
  });
  test("keep mail reports a daemon failure without claiming success", async () => {
    api.commitUnsubscribePreview.mockResolvedValueOnce({
      ok: false,
      result: {
        address: "news@digest.example",
        status: "failed",
        method: "None",
        message_count: 12,
        archived_count: 0,
        error: "The method changed; preview again",
      },
    });
    renderDialog();
    await vi.waitFor(() => expect(screen.getByTestId("unsubscribe-just")).toBeEnabled());
    fireEvent.click(screen.getByTestId("unsubscribe-just"));
    await vi.waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(toast.success).not.toHaveBeenCalled();
  });
  test("a cached preview cannot commit while a fresh preview is being fetched", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    qc.setQueryData(
      ["reading", "unsubscribe-preview", source.account_id, source.sender_email],
      preview,
    );
    let resolvePreview: ((result: UnsubscribePurgeResponse) => void) | undefined;
    api.unsubscribeAndClearSender.mockReturnValueOnce(
      new Promise((resolve) => {
        resolvePreview = resolve;
      }),
    );
    render(
      <QueryClientProvider client={qc}>
        <ReadingUnsubscribeDialog source={source} onClose={vi.fn<() => void>()} />
      </QueryClientProvider>,
    );
    expect(screen.getByTestId("unsubscribe-just")).toBeDisabled();
    expect(screen.getByTestId("unsubscribe-clear")).toBeDisabled();
    if (!resolvePreview) throw new Error("The preview request must start");
    resolvePreview(preview);
    await vi.waitFor(() => expect(screen.getByTestId("unsubscribe-just")).toBeEnabled());
  });
});
