import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import type { RecordLedger, RecordSubscriptions } from "./api";

const mocks = vi.hoisted(() => ({
  ledger: (() => {
    let current: RecordLedger | undefined;
    return {
      get current() {
        return current;
      },
      set current(value: RecordLedger | undefined) {
        current = value;
      },
    };
  })(),
  subscriptions: (() => {
    let current: RecordSubscriptions | undefined;
    return {
      get current() {
        return current;
      },
      set current(value: RecordSubscriptions | undefined) {
        current = value;
      },
    };
  })(),
  markChecked: vi.fn<() => void>(),
  markNotRecord: vi.fn<() => void>(),
  openMailDialog: vi.fn<() => void>(),
  toastInfo: vi.fn<() => void>(),
}));

vi.mock("@tanstack/react-router", () => ({ useParams: () => ({}) }));
vi.mock("@/features/places/PlaceLayout", () => ({
  PlaceLayout: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));
vi.mock("@/components/ModeFrame", () => ({
  ModeFrame: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ModeHeader: ({ children }: { children: React.ReactNode }) => <header>{children}</header>,
}));
vi.mock("@/components/ui/resizable", () => ({
  ResizableHandle: () => null,
  ResizablePanel: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  ResizablePanelGroup: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));
vi.mock("@/features/mailbox/readerNav", () => ({ useReaderNav: () => null }));
vi.mock("@/features/hints/useHint", () => ({
  useHint: () => ({ hint: undefined, dismiss: vi.fn<() => void>() }),
  useActiveHintDismiss: () => undefined,
}));
vi.mock("@/features/modes/api", () => ({ useModeGuide: () => ({ data: undefined }) }));
vi.mock("@/hooks/useDelayedPending", () => ({ useDelayedPending: () => "ready" }));
vi.mock("@/hooks/useMediaQuery", () => ({ SINGLE_PANE_QUERY: "", useMediaQuery: () => false }));
vi.mock("@/hooks/useSplitPane", () => ({
  useSplitPane: () => ({ groupProps: {}, otherPanelProps: {}, handleProps: {}, sidePanelProps: {} }),
}));
vi.mock("@/hooks/useShortcutScope", () => ({ useShortcutScope: vi.fn<() => void>() }));
vi.mock("@/features/archive/api", () => ({
  useAnswer: () => ({ data: undefined, isFetching: false }),
  useLedger: () => ({ data: mocks.ledger.current, isLoading: false, isError: false }),
  useRecord: () => ({ data: undefined }),
  useSubscriptions: () => ({ data: mocks.subscriptions.current }),
}));
vi.mock("@/features/archive/archiveVerbs", () => ({
  copyAmount: vi.fn<() => void>(),
  copyReference: vi.fn<() => void>(),
  copyText: vi.fn<() => void>(),
  markChecked: mocks.markChecked,
  markNotRecord: mocks.markNotRecord,
  openDocument: vi.fn<() => void>(),
}));
vi.mock("@/features/mail-actions/mailDialogStore", () => ({
  openMailDialog: mocks.openMailDialog,
}));
vi.mock("sonner", () => ({ toast: { info: mocks.toastInfo } }));
vi.mock("@/features/archive/FacetsPanel", () => ({ FacetsPanel: () => null }));
vi.mock("@/features/archive/RecordParts", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/archive/RecordParts")>()),
  AnswerCard: () => null,
  LedgerRow: ({ record }: { record: { title: string } }) => <li>{record.title}</li>,
  MatchesHeader: () => null,
  RecordCard: () => null,
}));

import { getController } from "@/lib/keys/controllers";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { ArchiveRoute } from "./ArchiveRoute";
import { recordFixture, subscriptionsFixture } from "./testing";

const originalScrollIntoView = HTMLElement.prototype.scrollIntoView;

beforeEach(() => {
  HTMLElement.prototype.scrollIntoView = vi.fn<() => void>();
});

afterEach(() => {
  mocks.markChecked.mockReset();
  mocks.markNotRecord.mockReset();
  mocks.openMailDialog.mockReset();
  mocks.toastInfo.mockReset();
  act(() => useUiPrefs.setState({ accountScope: null }));
  HTMLElement.prototype.scrollIntoView = originalScrollIntoView;
});

describe("Archive actions while subscriptions are displayed", () => {
  test("record actions cannot mutate or export the hidden ledger selection", async () => {
    const hiddenRecord = recordFixture({ id: "hidden-ledger-record", title: "Hidden ledger row" });
    mocks.ledger.current = {
      header: "Archive has one record",
      records: [hiddenRecord],
      months: [],
      matching: 1,
      total: 1,
      filter: {},
      facets: { checked: 0, has_pdf: 0, issuers: [], kinds: [], unchecked: 0, years: [] },
      coming_up: [],
    };
    mocks.subscriptions.current = subscriptionsFixture();

    render(<ArchiveRoute />);
    fireEvent.click(screen.getByTestId("subscriptions-chip"));
    expect((await screen.findAllByTestId("subscription-row"))[0]).toHaveTextContent("Netflix");
    expect(screen.getByTestId("subscriptions-chip")).toHaveAttribute("aria-pressed", "true");

    await waitFor(() => expect(getController("archive")).toBeDefined());
    const controller = getController("archive");
    act(() => {
      controller?.edit?.();
      controller?.check?.();
      controller?.dismiss?.();
      controller?.makeTodo?.();
      controller?.export?.();
      controller?.prevYear?.();
      controller?.nextYear?.();
    });

    expect(mocks.markChecked).not.toHaveBeenCalled();
    expect(mocks.markNotRecord).not.toHaveBeenCalled();
    expect(mocks.openMailDialog).not.toHaveBeenCalled();
    expect(mocks.toastInfo).toHaveBeenCalledTimes(7);
    expect(mocks.toastInfo).toHaveBeenCalledWith(
      "That works on a record: choose All to return to the ledger, or p for this issuer",
    );
  });
});
