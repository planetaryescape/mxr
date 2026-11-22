import { fireEvent, render, screen, } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { useComposeUi } from "@/features/compose/composeUiStore";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";

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

  test("Now's e and r act on a question's email, as on any Now row; m and x are not Now keys", async () => {
    useComposeUi.getState().closeCompose();
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    openSortingDetails();
    const row = screen.getAllByTestId("not-sure-row")[0]!;
    fireEvent.keyDown(row, { key: "m" });
    fireEvent.keyDown(row, { key: "x" });
    expect(fetchMock).not.toHaveBeenCalled();
    expect(useMailDialogs.getState().dialog).toBeNull();

    fireEvent.keyDown(row, { key: "r" });
    expect(useComposeUi.getState().intent).toMatchObject({
      kind: "reply",
      key: "compose:reply:msg-1",
    });
    useComposeUi.getState().closeCompose();

    fetchMock.mockResolvedValueOnce({
      kind: "ModeDone",
      dry_run: false,
      mutation_id: "m-1",
      items: [{ thread_id: "thread-1", mode: "updates", provider: "Gmail", copy: "Done." }],
    });
    fireEvent.keyDown(row, { key: "e" });
    expect(fetchMock).toHaveBeenCalledWith("/api/v1/mail/modes/updates/done", {
      method: "POST",
      body: { thread_ids: ["thread-1"], dry_run: false },
    });
    expect(await screen.findAllByTestId("not-sure-row")).toHaveLength(2);
  });

  test("X on a question opens the move picker as its answer", () => {
    useMailDialogs.getState().close();
    render(<ArrivalsView arrivals={withQuestions()} nowWaiting />);
    openSortingDetails();
    const row = screen.getAllByTestId("not-sure-row")[0]!;
    fireEvent.keyDown(row, { key: "X", shiftKey: true });
    const dialog = useMailDialogs.getState().dialog;
    expect(dialog?.kind).toBe("move-to-mode");
    if (dialog?.kind !== "move-to-mode") return;
    expect(dialog.subject.messageId).toBe("msg-1");
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
    view.rerender(<ArrivalsView arrivals={arrivals({ not_sure: [] })} nowWaiting />);
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    await new Promise((resolve) => setTimeout(resolve, 20));
    view.rerender(<ArrivalsView arrivals={one()} nowWaiting />);
    expect(await screen.findByTestId("not-sure-row")).toBeVisible();
  });
});
