import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import type { UnsubscribePurgeResponse } from "@/features/mailbox/api";

import type { ReadingSource } from "./api";
import { ReadingUnsubscribeDialog } from "./ReadingDialogs";

const api = vi.hoisted(() => ({
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
      <ReadingUnsubscribeDialog source={source} messageId="msg-9" onClose={onClose} />
    </QueryClientProvider>,
  );
  return onClose;
}

beforeEach(() => {
  api.unsubscribeAndClearSender.mockReset().mockResolvedValue(preview);
  api.unsubscribeFromSender.mockReset().mockResolvedValue({});
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

  test("just unsubscribe acts on the item's message and never archives", async () => {
    renderDialog();
    const just = screen.getByTestId("unsubscribe-just");
    await vi.waitFor(() => expect(just).toBeEnabled());
    fireEvent.click(just);
    await vi.waitFor(() =>
      expect(api.unsubscribeFromSender).toHaveBeenCalledWith({
        messageId: "msg-9",
        archive: false,
      }),
    );
    expect(api.unsubscribeAndClearSender).toHaveBeenCalledTimes(1);
  });

  test("a commits exactly the previewed token and clears the issues", async () => {
    renderDialog();
    await screen.findByText("Unsubscribe and clear 12 issues");
    await vi.waitFor(() => expect(screen.getByTestId("unsubscribe-clear")).toBeEnabled());
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "a" });
    await vi.waitFor(() =>
      expect(api.unsubscribeAndClearSender).toHaveBeenLastCalledWith({
        address: "news@digest.example",
        accountId: "acc-1",
        previewToken: "tok-1",
      }),
    );
    expect(api.unsubscribeFromSender).not.toHaveBeenCalled();
  });

  test("u just unsubscribes from the keyboard", async () => {
    renderDialog();
    await vi.waitFor(() => expect(screen.getByTestId("unsubscribe-just")).toBeEnabled());
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "u" });
    await vi.waitFor(() =>
      expect(api.unsubscribeFromSender).toHaveBeenCalledWith({
        messageId: "msg-9",
        archive: false,
      }),
    );
  });
});
