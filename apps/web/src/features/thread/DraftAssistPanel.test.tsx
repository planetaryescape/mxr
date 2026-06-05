/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

const draftAssist = vi.hoisted(() => vi.fn<(input: unknown) => Promise<unknown>>());
vi.mock("@/features/mailbox/api", () => ({ draftAssistThread: draftAssist }));
vi.mock("sonner", () => ({
  toast: {
    error: vi.fn<(message: string, opts?: unknown) => void>(),
    success: vi.fn<(message: string, opts?: unknown) => void>(),
  },
}));

import { DraftAssistPanel } from "./DraftAssistPanel";

function renderWithClient(node: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{node}</QueryClientProvider>);
}

afterEach(() => vi.clearAllMocks());

describe("DraftAssistPanel", () => {
  test("shows the matched-tone chip after generating", async () => {
    draftAssist.mockResolvedValue({
      body: "Sure, Friday works.",
      context_note: "Matched to alice@example.com (casual, short)",
    });
    renderWithClient(<DraftAssistPanel threadId="t1" />);

    fireEvent.change(screen.getByLabelText("Draft instruction"), {
      target: { value: "reply yes" },
    });
    fireEvent.click(screen.getByRole("button", { name: /generate/i }));

    expect(
      await screen.findByText("Matched to alice@example.com (casual, short)"),
    ).toBeInTheDocument();
  });

  test("drafts with auto tone (no override) by default", async () => {
    draftAssist.mockResolvedValue({ body: "ok" });
    renderWithClient(<DraftAssistPanel threadId="t1" />);

    fireEvent.change(screen.getByLabelText("Draft instruction"), { target: { value: "reply" } });
    fireEvent.click(screen.getByRole("button", { name: /generate/i }));

    await screen.findByLabelText("Draft preview");
    expect(draftAssist).toHaveBeenCalledWith({ threadId: "t1", instruction: "reply" });
  });
});
