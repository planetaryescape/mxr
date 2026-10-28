import { fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { useKeyScope } from "@/state/keyScopeStore";

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

describe("Now's arrivals line", () => {
  test("keeps provisional placement counts in the existing disclosure", () => {
    render(
      <ArrivalsView
        nowWaiting
        arrivals={arrivals({
          classification_report: {
            policy_version: "r2-1",
            examined: 1000,
            missing: 0,
            stale: 0,
            pending_body: 2,
            fallback: 2,
            truncated: true,
            clusters: [
              {
                home: "messages",
                who: "person",
                rule: "person",
                count: 2,
                examples: ["message-1"],
                facts: {
                  sender_shape: "person-like address",
                  has_list_id: false,
                  has_unsubscribe: false,
                  auto_generated: false,
                  bulk: false,
                  templated_sender: false,
                  written_to: false,
                  body_cached: false,
                  recipients: 1,
                  addressed: true,
                },
              },
            ],
          },
        })}
      />,
    );
    expect(screen.getByTestId("classification-report")).not.toBeVisible();
    openSortingDetails();
    expect(screen.getByTestId("classification-report")).toHaveTextContent(
      "2 emails placed provisionally in this sample.",
    );
    expect(screen.getByTestId("classification-report")).toHaveTextContent(
      "2 need more of the email.",
    );
  });

  test("keeps the headline and correction count visible, with sorting details closed", () => {
    render(
      <ArrivalsView
        arrivals={arrivals({ not_sure: [notSure()], not_sure_hint: "Rules differed." })}
        nowWaiting
      />,
    );

    expect(screen.getByTestId("arrivals-summary")).toHaveTextContent("50 arrived");
    expect(screen.getByTestId("arrivals-summary")).toHaveTextContent("1 email needs sorting");
    expect(screen.getByTestId("arrivals")).not.toHaveAttribute("open");
    expect(screen.getByTestId("arrivals-line")).not.toBeVisible();
    expect(screen.getByTestId("not-sure-hint")).not.toBeVisible();

    openSortingDetails();
    expect(screen.getByTestId("arrivals-line")).toBeVisible();
    expect(screen.getByTestId("not-sure-hint")).toBeVisible();
  });

  test("is the daemon's sentence with each count a link to those emails in its window", () => {
    render(<ArrivalsView arrivals={arrivals()} nowWaiting />);
    openSortingDetails();
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
    openSortingDetails();
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
    openSortingDetails();
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

  test("asks with one choice per mode and a hint under the first question only", () => {
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    openSortingDetails();
    expect(screen.getByRole("heading", { name: /wasn't sure/ })).toBeVisible();
    expect(screen.getAllByTestId("not-sure-row")).toHaveLength(2);
    expect(screen.getAllByTestId("not-sure-hint")).toHaveLength(1);
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
    // The answered question leaves at once, and with it the hint.
    expect(screen.getAllByTestId("not-sure-row")).toHaveLength(1);
    expect(screen.queryByTestId("not-sure-hint")).toBeNull();

    const ask = await screen.findByTestId("not-sure-sender-ask");
    expect(ask).toHaveTextContent("Always for Maya Ortiz?");
    fetchMock.mockResolvedValueOnce({
      kind: "MessageMoved",
      outcome: outcome({ to: "messages", sender: true, ask_sender: null }),
    });
    // Enter is yes, as y is, in every yes/no question (blueprint 35).
    fireEvent.keyDown(ask, { key: "Enter" });
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
    openSortingDetails();
    fireEvent.click(screen.getAllByTestId("not-sure-updates")[0]!);
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    await Promise.resolve();
    expect(screen.queryByTestId("not-sure-sender-ask")).toBeNull();
  });

  test("X on a question opens the move picker as its answer; the old letters say so", () => {
    useMailDialogs.getState().close();
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    openSortingDetails();
    const row = screen.getAllByTestId("not-sure-row")[0]!;
    fireEvent.keyDown(row, { key: "m" });
    expect(fetchMock).not.toHaveBeenCalled();
    expect(useKeyScope.getState().moved).toEqual({ label: "Move", key: "X" });

    fireEvent.keyDown(row, { key: "X", shiftKey: true });
    const dialog = useMailDialogs.getState().dialog;
    expect(dialog?.kind).toBe("move-to-mode");
    if (dialog?.kind !== "move-to-mode") return;
    expect(dialog.subject.messageId).toBe("msg-1");
    // The picker's answer is the question's: sent with `source: "not_sure"`.
    fetchMock.mockResolvedValueOnce({ kind: "MessageMoved", outcome: outcome({ to: "todo" }) });
    dialog.subject.notSure?.answer("todo", false);
    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/messages/msg-1/move", {
      method: "POST",
      body: { mode: "todo", sender: false, dry_run: false, source: "not_sure" },
    });
    useMailDialogs.getState().close();
  });

  test("n declines the sender question", async () => {
    fetchMock.mockResolvedValueOnce({ kind: "MessageMoved", outcome: outcome() });
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    openSortingDetails();
    fireEvent.click(screen.getAllByTestId("not-sure-reading")[0]!);
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
    openSortingDetails();
    fireEvent.click(screen.getByTestId("not-sure-messages"));
    expect(screen.queryByTestId("not-sure-row")).toBeNull();
    expect(await screen.findByTestId("not-sure-row")).toBeVisible();
    expect(screen.queryByTestId("not-sure-sender-ask")).toBeNull();
  });

  test("the sender question survives the refetch that empties the list", async () => {
    fetchMock.mockResolvedValueOnce({ kind: "MessageMoved", outcome: outcome({ to: "messages" }) });
    const view = render(<ArrivalsView arrivals={one()} nowWaiting />);
    openSortingDetails();
    fireEvent.click(screen.getByTestId("not-sure-messages"));
    fireEvent.click(screen.getByTestId("arrivals-summary"));
    expect(screen.getByTestId("arrivals")).not.toHaveAttribute("open");
    // The answer refreshes the line: no questions left.
    view.rerender(<ArrivalsView arrivals={arrivals({ not_sure: [] })} nowWaiting />);
    const ask = await screen.findByTestId("not-sure-sender-ask");
    expect(ask).toHaveTextContent("Always for Maya Ortiz?");
    expect(ask).not.toBeVisible();
    openSortingDetails();
    expect(ask).toBeVisible();
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
    // The answer refreshes the line without it, then Undo lists it again.
    view.rerender(<ArrivalsView arrivals={arrivals({ not_sure: [] })} nowWaiting />);
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    // Let the move settle (its toast, then the refresh) before Undo.
    await new Promise((resolve) => setTimeout(resolve, 20));
    view.rerender(<ArrivalsView arrivals={one()} nowWaiting />);
    expect(await screen.findByTestId("not-sure-row")).toBeVisible();
  });
});
