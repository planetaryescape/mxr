import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import {
  cancelInviteResponse,
  respondToInvite,
  RSVP_HOLD_MS,
  useInviteRsvp,
} from "./inviteResponse";

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

  test("a second choice inside the window replaces the first", async () => {
    client.apiFetch.mockResolvedValue({});

    respondToInvite("msg-3", "accept");
    vi.advanceTimersByTime(RSVP_HOLD_MS - 100);
    respondToInvite("msg-3", "tentative");
    await vi.advanceTimersByTimeAsync(RSVP_HOLD_MS);

    expect(client.apiFetch).toHaveBeenCalledTimes(1);
    expect(client.apiFetch).toHaveBeenCalledWith(REPLY, {
      method: "POST",
      body: { message_id: "msg-3", action: "tentative" },
    });
  });

  test("cancelling once the reply has gone out reports nothing to cancel", async () => {
    client.apiFetch.mockResolvedValue({});
    respondToInvite("msg-4", "accept");
    await vi.advanceTimersByTimeAsync(RSVP_HOLD_MS);

    expect(cancelInviteResponse("msg-4")).toBe(false);
  });

  test("a failed send shows an error with Retry that holds and sends again", async () => {
    client.apiFetch.mockRejectedValueOnce(new Error("calendar offline")).mockResolvedValueOnce({});

    respondToInvite("msg-5", "decline");
    await vi.advanceTimersByTimeAsync(RSVP_HOLD_MS);

    expect(toast.error).toHaveBeenCalledWith(
      "RSVP failed",
      expect.objectContaining({ description: "calendar offline" }),
    );
    expect(useInviteRsvp.getState().pending).toEqual({});

    const [, options] = toast.error.mock.calls[0]!;
    expect(options?.action?.label).toBe("Retry");
    options?.action?.onClick();
    await vi.advanceTimersByTimeAsync(RSVP_HOLD_MS);

    expect(client.apiFetch).toHaveBeenCalledTimes(2);
    expect(toast.success).toHaveBeenCalledWith("Declined", expect.anything());
  });
});
