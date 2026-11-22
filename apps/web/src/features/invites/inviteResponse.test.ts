import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { respondToInvite, RSVP_HOLD_MS, useInviteRsvp } from "./inviteResponse";

const client = vi.hoisted(() => ({
  apiFetch: vi.fn<(path: string, init?: unknown) => Promise<unknown>>(),
}));
vi.mock("@/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/client")>()),
  apiFetch: client.apiFetch,
}));

interface ToastOptions {
  id?: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}
const toast = vi.hoisted(() =>
  Object.assign(vi.fn<(message: string, options?: ToastOptions) => void>(), {
    success: vi.fn<(message: string, options?: ToastOptions) => void>(),
    error: vi.fn<(message: string, options?: ToastOptions) => void>(),
    loading: vi.fn<(message: string, options?: ToastOptions) => void>(),
  }),
);
vi.mock("sonner", () => ({ toast }));

const REPLY = "/api/v1/mail/actions/invite/reply";

beforeEach(() => {
  vi.useFakeTimers();
  useInviteRsvp.setState({ pending: {}, sending: {} });
});

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("respondToInvite", () => {
  test("holds the reply, then sends it when the window closes", async () => {
    client.apiFetch.mockResolvedValue({});

    respondToInvite("msg-1", "accept");

    expect(toast).toHaveBeenCalledWith(
      "Accepting…",
      expect.objectContaining({ action: expect.objectContaining({ label: "Undo" }) }),
    );
    expect(useInviteRsvp.getState().pending).toEqual({ "msg-1": "accept" });
    vi.advanceTimersByTime(RSVP_HOLD_MS - 1);
    expect(client.apiFetch).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);

    expect(client.apiFetch).toHaveBeenCalledWith(REPLY, {
      method: "POST",
      body: { message_id: "msg-1", action: "accept" },
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Accepted",
      expect.objectContaining({ id: "rsvp-msg-1" }),
    );
    expect(useInviteRsvp.getState()).toEqual({ pending: {}, sending: {} });
  });

  test("Undo within the window sends nothing", async () => {
    respondToInvite("msg-2", "decline");
    vi.advanceTimersByTime(RSVP_HOLD_MS / 2);

    const [, options] = toast.mock.calls[0]!;
    options?.action?.onClick();
    await vi.advanceTimersByTimeAsync(RSVP_HOLD_MS * 2);

    expect(client.apiFetch).not.toHaveBeenCalled();
    expect(toast.success).toHaveBeenCalledWith("RSVP cancelled", expect.anything());
    expect(useInviteRsvp.getState().pending).toEqual({});
  });
});
