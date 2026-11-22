import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { ArrivalsView } from "./ArrivalsLine";
import { arrivals, notSure, outcome } from "./testing";
import { resetHintsForTests } from "./useTrustHint";

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    children,
    search,
    ...rest
  }: {
    children: ReactNode;
    to: string;
    search?: Record<string, string>;
  }) => (
    <a {...rest} href={`${rest.to}?${new URLSearchParams(search)}`}>
      {children}
    </a>
  ),
}));

const fetchMock = vi.hoisted(() => vi.fn<(path: string, opts?: unknown) => Promise<unknown>>());
vi.mock("@/api/client", () => ({ apiFetch: fetchMock }));
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(...args: unknown[]) => void>(),
    error: vi.fn<(...args: unknown[]) => void>(),
    info: vi.fn<(...args: unknown[]) => void>(),
    dismiss: vi.fn<(...args: unknown[]) => void>(),
  },
}));

beforeEach(() => {
  fetchMock.mockReset();
  resetHintsForTests();
});

function openSortingDetails() {
  fireEvent.click(screen.getByTestId("arrivals-summary"));
}

describe("Not sure", () => {
  const withQuestions = () =>
    arrivals({
      not_sure: [
        notSure(),
        notSure({ message_id: "msg-2", line: "Sam copied you. Updates for now." }),
      ],
      not_sure_line: "2 emails I wasn't sure about. Where should these go?",
      not_sure_hint: "Two of mxr's rules disagreed about this one.",
    });

  test("a choice answers it, then asks once whether to do the same for the sender", async () => {
    fetchMock.mockResolvedValueOnce({ kind: "MessageMoved", outcome: outcome({ to: "messages" }) });
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    openSortingDetails();
    fireEvent.click(screen.getAllByTestId("not-sure-messages")[0]!);

    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/messages/msg-1/move", {
      method: "POST",
      body: { mode: "messages", sender: false, dry_run: false, source: "not_sure" },
    });
    expect(screen.getAllByTestId("not-sure-row")).toHaveLength(1);
    expect(screen.queryByTestId("not-sure-hint")).toBeNull();

    const ask = await screen.findByTestId("not-sure-sender-ask");
    expect(ask).toHaveTextContent("Always for Maya Ortiz?");
    fetchMock.mockResolvedValueOnce({
      kind: "MessageMoved",
      outcome: outcome({ to: "messages", sender: true, ask_sender: null }),
    });
    fireEvent.keyDown(ask, { key: "Enter" });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/mail/messages/msg-1/move", {
      method: "POST",
      body: { mode: "messages", sender: true, dry_run: false },
    });
    expect(screen.queryByTestId("not-sure-sender-ask")).toBeNull();
  });
});

describe("Not sure: failures and the last answer", () => {
  const one = () =>
    arrivals({ not_sure: [notSure()], not_sure_line: "1 email I wasn't sure about." });

  test("a move that fails brings the question back", async () => {
    fetchMock.mockRejectedValueOnce(new Error("daemon said no"));
    render(<ArrivalsView arrivals={one()} nowWaiting />);
    openSortingDetails();
    fireEvent.click(screen.getByTestId("not-sure-messages"));
    expect(screen.queryByTestId("not-sure-row")).toBeNull();
    expect(await screen.findByTestId("not-sure-row")).toBeVisible();
    expect(screen.queryByTestId("not-sure-sender-ask")).toBeNull();
  });

  test("Undo after the last answer brings the question back", async () => {
    fetchMock.mockResolvedValueOnce({
      kind: "MessageMoved",
      outcome: outcome({ to: "messages", from: "messages", correction_id: null }),
    });
    const view = render(<ArrivalsView arrivals={one()} nowWaiting />);
    openSortingDetails();
    fireEvent.click(screen.getByTestId("not-sure-messages"));
    expect(screen.queryByTestId("not-sure-row")).toBeNull();
    view.rerender(<ArrivalsView arrivals={arrivals({ not_sure: [] })} nowWaiting />);
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    await new Promise((resolve) => setTimeout(resolve, 20));
    view.rerender(<ArrivalsView arrivals={one()} nowWaiting />);
    expect(await screen.findByTestId("not-sure-row")).toBeVisible();
  });
});
