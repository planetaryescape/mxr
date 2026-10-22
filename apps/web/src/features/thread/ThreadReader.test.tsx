/* @vitest-environment jsdom */

import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ThreadResponse } from "@/features/mailbox/types";
import { createQueryClient } from "@/lib/queryClient";
import { useMailboxPane } from "@/state/mailboxPaneStore";

import { ThreadReader } from "./ThreadReader";

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  Link: ({ children }: { children: ReactNode }) => <a href="/m/inbox">{children}</a>,
  useNavigate: () => vi.fn<() => void>(),
  useRouterState: ({ select }: { select: (state: unknown) => unknown }) =>
    select({ location: { pathname: "/m/inbox/thread-1", search: {} } }),
}));
vi.mock("@/features/llm/useLlmStatus", () => ({
  llmPolicyKey: () => "off",
  useLlmStatus: () => ({ enabled: false, data: undefined }),
}));

const thread: ThreadResponse = {
  thread: {
    account_id: "acct",
    id: "thread-1",
    latest_date: "2026-10-06T16:00:00Z",
    message_count: 0,
    participants: [],
    snippet: "",
    subject: "Contract renewal",
    unread_count: 0,
  },
  messages: [],
  bodies: [],
};

afterEach(() => {
  act(() => useMailboxPane.getState().setActivePane("mailbox"));
});

const client = createQueryClient();

function Harness({ open }: { open: boolean }) {
  return (
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <ThreadReader data={thread} />
      </TooltipProvider>
      {/* Opened by a shortcut: no trigger to hand focus back to. */}
      <Dialog open={open}>
        <DialogContent aria-describedby={undefined}>
          <DialogTitle>Keys</DialogTitle>
        </DialogContent>
      </Dialog>
    </QueryClientProvider>
  );
}

describe("ThreadReader", () => {
  test("takes focus back when an overlay closes while it drives the keys", async () => {
    act(() => useMailboxPane.getState().setActivePane("reader"));
    const { rerender } = render(<Harness open />);
    const dialog = await screen.findByRole("dialog");
    await waitFor(() => expect(dialog).toContainElement(document.activeElement as HTMLElement));

    fireEvent.keyDown(dialog, { key: "Escape" });
    rerender(<Harness open={false} />);

    await waitFor(() =>
      expect(screen.getByRole("article", { name: "Conversation: Contract renewal" })).toHaveFocus(),
    );
  });
});
