/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { CommandPaletteMount } from "./CommandPalette";
import { useModals } from "@/state/modalStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
  pathname: "/m/inbox/thread-1",
}));

const accountsApi = vi.hoisted(() => ({
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
}));

const diagnosticsApi = vi.hoisted(() => ({
  backfillSemantic: vi.fn<() => Promise<unknown>>(),
}));

const mailboxApi = vi.hoisted(() => ({
  fetchShell: vi.fn<() => Promise<unknown>>(),
  listCommitments:
    vi.fn<
      (input: {
        accountId: string;
        email?: string;
        status?: "open" | "resolved" | "expired";
      }) => Promise<unknown>
    >(),
}));

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useNavigate: () => router.navigate,
    useRouterState: ({
      select,
    }: {
      select: (state: { location: { pathname: string } }) => unknown;
    }) => select({ location: { pathname: router.pathname } }),
  };
});

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: accountsApi.fetchAccounts,
}));

vi.mock("@/features/diagnostics/api", () => ({
  backfillSemantic: diagnosticsApi.backfillSemantic,
}));

vi.mock("@/features/mailbox/api", () => ({
  fetchShell: mailboxApi.fetchShell,
  listCommitments: mailboxApi.listCommitments,
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn<(message: string, options?: unknown) => void>(),
    success: vi.fn<(message: string, options?: unknown) => void>(),
  },
}));

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("CommandPaletteMount", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "ResizeObserver",
      class ResizeObserver {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
    Object.defineProperty(window.HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn<() => void>(),
    });
    useModals.setState({ commandPaletteOpen: true, rightRail: null });
    accountsApi.fetchAccounts.mockResolvedValue({
      accounts: [
        {
          account_id: "account-1",
          name: "Personal",
          email: "me@example.com",
          provider_kind: "fake",
          enabled: true,
          is_default: true,
        },
      ],
    });
    diagnosticsApi.backfillSemantic.mockResolvedValue({ ok: true });
    mailboxApi.fetchShell.mockResolvedValue({ sidebar: { sections: [] } });
    mailboxApi.listCommitments.mockResolvedValue({ commitments: [] });
  });

  afterEach(() => {
    useModals.setState({ commandPaletteOpen: false, rightRail: null });
    vi.clearAllMocks();
    vi.unstubAllGlobals();
  });

  test("surfaces platform quick actions and settings routes", () => {
    renderWithQueryClient(<CommandPaletteMount />);

    expect(screen.getByText("Draft to...")).toBeVisible();
    expect(screen.getByText("Show commitments...")).toBeVisible();
    expect(screen.getByText("Backfill semantic now")).toBeVisible();
    expect(screen.getByText("Voice settings")).toBeVisible();
    expect(screen.getByText("LLM settings")).toBeVisible();
  });
});
