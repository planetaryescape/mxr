import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { ModeChip, ModeChipScope } from "./ModeChip";

const fetchMock = vi.hoisted(() => vi.fn<(path: string, opts?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));

beforeEach(() => fetchMock.mockReset());

describe("Inbox's mode chip", () => {
  test("names the mode quietly, with the reason on hover", async () => {
    fetchMock.mockResolvedValueOnce({
      kind: "ArrivalModes",
      items: [
        {
          account_id: "a",
          message_id: "msg-1",
          thread_id: "t",
          sender_email: "no-reply@shop.example",
          subject: "Your order",
          date: "2026-10-07T08:00:00Z",
          first_seen_at: "2026-10-07T08:00:00Z",
          arrived_in: "updates",
          bucket: "updates",
          reason: "automated sender",
          chip: "→ Updates · automated sender",
        },
      ],
    });
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ModeChipScope enabled messageIds={["msg-1", "msg-2"]}>
          <ModeChip messageId="msg-1" />
          <ModeChip messageId="msg-2" />
        </ModeChipScope>
      </QueryClientProvider>,
    );
    const chip = await screen.findByTestId("mode-chip");
    expect(chip).toHaveTextContent("Updates");
    expect(chip).toHaveAttribute("title", "→ Updates · automated sender");
    expect(screen.getAllByTestId("mode-chip")).toHaveLength(1);
    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/arrivals/modes", {
      method: "POST",
      body: { message_ids: ["msg-1", "msg-2"] },
    });
  });

  test("a list without chips asks nothing", () => {
    render(
      <ModeChipScope enabled={false} messageIds={["msg-1"]}>
        <ModeChip messageId="msg-1" />
      </ModeChipScope>,
    );
    expect(screen.queryByTestId("mode-chip")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
