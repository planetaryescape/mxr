/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { ListWithReader } from "./ListWithReader";
import type { MessageGroupView, MessageRowView } from "./types";

const navigate = vi.hoisted(() => vi.fn<(options: unknown) => void>());
const advance = vi.hoisted(() => ({ ids: [] as readonly string[] }));

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  Outlet: () => null,
  useNavigate: () => navigate,
  useParams: () => ({}),
}));
vi.mock("@/hooks/useAdvanceOnRemoval", () => ({
  useAdvanceOnRemoval: (options: { ids: readonly string[] }) => {
    advance.ids = options.ids;
    return () => () => {};
  },
  useRouteStillShows: () => () => true,
  useRouteAccount: () => "B",
}));
vi.mock("@/hooks/useMediaQuery", () => ({
  SINGLE_PANE_QUERY: "",
  useMediaQuery: () => false,
}));
vi.mock("@/hooks/useSplitPane", () => ({
  useSplitPane: () => ({
    groupProps: {},
    sidePanelProps: {},
    handleProps: {},
    otherPanelProps: {},
  }),
}));
vi.mock("@/components/ModeFrame", () => ({
  ModeFrame: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/components/PageFrame", async () => ({
  FramePanesContext: (await import("react")).createContext({}),
  PageFrame: () => null,
  READER_TOOLBAR_HEIGHT: 0,
}));
vi.mock("@/components/ui/resizable", () => ({
  ResizablePanelGroup: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizablePanel: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizableHandle: () => null,
}));
vi.mock("./MailboxList", () => ({
  MailboxList: ({
    groups,
    onOpenRow,
  }: {
    groups: MessageGroupView[];
    onOpenRow: (row: MessageRowView, options: { focusReader: boolean }) => void;
  }) => (
    <div>
      {groups.flatMap((group) =>
        group.rows.map((listed) => (
          <button
            key={listed.id}
            type="button"
            onClick={() => onOpenRow(listed, { focusReader: true })}
          >
            {listed.subject}
          </button>
        )),
      )}
    </div>
  ),
}));

function row(account: string): MessageRowView {
  return {
    id: `m-${account}`,
    kind: "thread",
    thread_id: "shared",
    account_id: account,
    provider_id: "",
    sender: "",
    subject: `In ${account}`,
    snippet: "",
    date: "",
    date_label: "",
    date_full: "",
    date_relative: "",
    unread: false,
    starred: false,
    has_attachments: false,
  };
}

const groups: MessageGroupView[] = [
  { id: "all", label: "All", rows: [row("A"), row("B")], count: 2 },
];

beforeEach(() => navigate.mockReset());

describe("opening a row of a thread id held by two accounts", () => {
  test("opening B's row puts B's account in the route, so the reader fetches B's copy", () => {
    render(
      <ListWithReader
        basePath="/m/inbox"
        title="Inbox"
        groups={groups}
        scopeKey="inbox"
        status={{
          isLoading: false,
          isError: false,
          error: null,
          refetch: () => {},
          data: {},
        }}
        renderRow={() => null}
        empty={null}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "In B" }));
    const call = navigate.mock.calls[0]?.[0] as {
      to: string;
      search: (previous: Record<string, unknown>) => Record<string, unknown>;
    };
    expect(call.to).toBe("/m/inbox/shared");
    expect(call.search({})).toEqual({ account: "B" });
  });

  test("the list advances over both accounts' rows as two", () => {
    render(
      <ListWithReader
        basePath="/m/inbox"
        title="Inbox"
        groups={groups}
        scopeKey="inbox"
        status={{
          isLoading: false,
          isError: false,
          error: null,
          refetch: () => {},
          data: {},
        }}
        renderRow={() => null}
        empty={null}
      />,
    );
    expect(advance.ids).toHaveLength(2);
    expect(new Set(advance.ids).size).toBe(2);
  });
});
