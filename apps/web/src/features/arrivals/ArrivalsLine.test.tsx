import { fireEvent, render, screen, within } from "@testing-library/react";
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

describe("Now's arrivals line", () => {
  test("is the daemon's sentence with each count a link to those emails in its window", () => {
    render(<ArrivalsView arrivals={arrivals()} nowWaiting />);
    const line = screen.getByTestId("arrivals-line");
    expect(line).toHaveTextContent(
      "Since 08:12: 50 arrived. 8 Messages · 10 Updates · 31 Reading · 1 spam. Also 2 in To do.",
    );
    const reading = within(line).getByRole("link", { name: "31 Reading" });
    expect(reading).toHaveAttribute(
      "href",
      "/arrivals?bucket=reading&since=2026-10-07T08%3A12%3A00Z&until=2026-10-07T12%3A00%3A01Z",
    );
    expect(within(line).getAllByRole("link")).toHaveLength(5);
  });

  test("says Clear when nothing on Now waits", () => {
    render(<ArrivalsView arrivals={arrivals()} nowWaiting={false} />);
    expect(screen.getByTestId("arrivals-line")).toHaveTextContent(
      "Clear. All 50 emails since 08:12 are accounted for.",
    );
  });

  test("shows the track record only when the daemon sends one", () => {
    const { rerender } = render(<ArrivalsView arrivals={arrivals()} nowWaiting />);
    expect(screen.queryByTestId("arrivals-track-record")).toBeNull();
    rerender(
      <ArrivalsView
        arrivals={arrivals({ track_record: "Last week mxr sorted 310 emails; you moved 2." })}
        nowWaiting
      />,
    );
    expect(screen.getByTestId("arrivals-track-record")).toHaveTextContent(
      "Last week mxr sorted 310 emails; you moved 2.",
    );
  });
});

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

  test("asks with one key per mode and a hint under the first question only", () => {
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    expect(screen.getByRole("heading", { name: /wasn't sure/ })).toBeVisible();
    expect(screen.getAllByTestId("not-sure-row")).toHaveLength(2);
    expect(screen.getAllByTestId("not-sure-hint")).toHaveLength(1);
  });

  test("a key answers it, then asks once whether to do the same for the sender", async () => {
    fetchMock.mockResolvedValueOnce({ kind: "MessageMoved", outcome: outcome({ to: "messages" }) });
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    fireEvent.keyDown(screen.getAllByTestId("not-sure-row")[0]!, { key: "m" });

    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/messages/msg-1/move", {
      method: "POST",
      body: { mode: "messages", sender: false, dry_run: false, source: "not_sure" },
    });
    // The answered question leaves at once, and with it the hint.
    expect(screen.getAllByTestId("not-sure-row")).toHaveLength(1);
    expect(screen.queryByTestId("not-sure-hint")).toBeNull();

    const ask = await screen.findByTestId("not-sure-sender-ask");
    expect(ask).toHaveTextContent("Always for Maya Ortiz?");
    fetchMock.mockResolvedValueOnce({
      kind: "MessageMoved",
      outcome: outcome({ to: "messages", sender: true, ask_sender: null }),
    });
    fireEvent.keyDown(ask, { key: "y" });
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/mail/messages/msg-1/move", {
      method: "POST",
      body: { mode: "messages", sender: true, dry_run: false },
    });
    expect(screen.queryByTestId("not-sure-sender-ask")).toBeNull();
  });

  test("keeping it where it is answers without asking about the sender", async () => {
    fetchMock.mockResolvedValueOnce({
      kind: "MessageMoved",
      outcome: outcome({
        from: "updates",
        to: "updates",
        copy: "Kept in Updates.",
        ask_sender: null,
      }),
    });
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    fireEvent.click(screen.getAllByTestId("not-sure-updates")[0]!);
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    await Promise.resolve();
    expect(screen.queryByTestId("not-sure-sender-ask")).toBeNull();
  });

  test("n declines the sender question", async () => {
    fetchMock.mockResolvedValueOnce({ kind: "MessageMoved", outcome: outcome() });
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    fireEvent.keyDown(screen.getAllByTestId("not-sure-row")[0]!, { key: "r" });
    fireEvent.keyDown(await screen.findByTestId("not-sure-sender-ask"), { key: "n" });
    expect(screen.queryByTestId("not-sure-sender-ask")).toBeNull();
    expect(fetchMock).toHaveBeenCalledOnce();
  });
});

describe("Not sure: failures and the last answer", () => {
  const one = () =>
    arrivals({ not_sure: [notSure()], not_sure_line: "1 email I wasn't sure about." });

  test("a move that fails brings the question back", async () => {
    fetchMock.mockRejectedValueOnce(new Error("daemon said no"));
    render(<ArrivalsView arrivals={one()} nowWaiting />);
    fireEvent.keyDown(screen.getByTestId("not-sure-row"), { key: "m" });
    expect(screen.queryByTestId("not-sure-row")).toBeNull();
    expect(await screen.findByTestId("not-sure-row")).toBeVisible();
    expect(screen.queryByTestId("not-sure-sender-ask")).toBeNull();
  });

  test("the sender question survives the refetch that empties the list", async () => {
    fetchMock.mockResolvedValueOnce({ kind: "MessageMoved", outcome: outcome({ to: "messages" }) });
    const view = render(<ArrivalsView arrivals={one()} nowWaiting />);
    fireEvent.keyDown(screen.getByTestId("not-sure-row"), { key: "m" });
    // The answer refreshes the line: no questions left.
    view.rerender(<ArrivalsView arrivals={arrivals({ not_sure: [] })} nowWaiting />);
    expect(await screen.findByTestId("not-sure-sender-ask")).toHaveTextContent(
      "Always for Maya Ortiz?",
    );
  });
});
