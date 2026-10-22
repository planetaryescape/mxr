/* @vitest-environment jsdom */

import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { getRegistry, snapshotActionContext } from "@/lib/actions";
import { installKeyDispatcher } from "@/lib/keys/dispatcher";
import { useModals } from "@/state/modalStore";

import { ArrivalsRoute } from "./ArrivalsRoute";
import type { ArrivalItem } from "./types";

const navigate = vi.hoisted(() => vi.fn<(options: { to: string }) => void>());
const moves = vi.hoisted(() => ({
  openMovePicker: vi.fn<(subject: { messageId: string }, sender?: boolean) => void>(),
  answerPendingSenderAsk: vi.fn<() => boolean>(() => false),
}));

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: unknown }) => children,
  useNavigate: () => navigate,
}));
vi.mock("./moves", () => moves);

function item(id: string): ArrivalItem {
  return {
    account_id: "acct-1",
    message_id: `msg-${id}`,
    thread_id: `thread-${id}`,
    sender_email: `${id}@example.com`,
    subject: `Subject ${id}`,
    date: "2026-10-07T09:00:00Z",
    first_seen_at: "2026-10-07T09:00:00Z",
    arrived_in: "updates",
    bucket: "updates",
    chip: "→ Updates",
  };
}

vi.mock("./api", () => ({
  useArrivalList: () => ({
    data: { since: "", until: "", total: 2, items: [item("a"), item("b")] },
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn<() => void>(),
  }),
}));

let uninstall: () => void = () => {};

describe("ArrivalsRoute keys", () => {
  beforeEach(() => {
    // The app's one dispatcher: arrival keys reach the page through the
    // registry's "arrivals" scope, not a listener on the rows.
    uninstall = installKeyDispatcher(window, {
      registry: getRegistry(),
      context: snapshotActionContext,
    });
  });

  afterEach(() => {
    uninstall();
    vi.clearAllMocks();
    useModals.getState().setHelpOpen(false);
  });

  test("j, Enter, X and K act on the row under the cursor", () => {
    render(<ArrivalsRoute />);
    const rows = screen.getAllByTestId("arrival-row");
    expect(rows[0]?.getAttribute("aria-current")).toBe("true");

    fireEvent.keyDown(window, { key: "j" });
    expect(rows[1]?.getAttribute("aria-current")).toBe("true");

    fireEvent.keyDown(window, { key: "X" });
    expect(moves.openMovePicker).toHaveBeenLastCalledWith(
      expect.objectContaining({ messageId: "msg-b" }),
      false,
    );
    fireEvent.keyDown(window, { key: "K" });
    expect(moves.openMovePicker).toHaveBeenLastCalledWith(
      expect.objectContaining({ messageId: "msg-b" }),
      true,
    );

    fireEvent.keyDown(window, { key: "k" });
    fireEvent.keyDown(window, { key: "Enter" });
    expect(navigate).toHaveBeenCalledWith({ to: "/m/archive/thread-a" });
  });

  test("a focused row keeps the global keys working", () => {
    render(<ArrivalsRoute />);
    const row = screen.getAllByTestId("arrival-row")[1];
    expect(row?.hasAttribute("data-keys-owner")).toBe(false);
    act(() => row?.focus());

    fireEvent.keyDown(row!, { key: "?" });
    expect(useModals.getState().helpOpen).toBe(true);

    fireEvent.keyDown(row!, { key: "X" });
    expect(moves.openMovePicker).toHaveBeenLastCalledWith(
      expect.objectContaining({ messageId: "msg-b" }),
      false,
    );
  });
});
