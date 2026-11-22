/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DiagnosticsRoute, } from "./DiagnosticsRoute";

const diagnosticsApi = vi.hoisted(() => ({
  backfillSemantic: vi.fn<() => Promise<unknown>>(),
  fetchAdminStatus: vi.fn<() => Promise<unknown>>(),
  fetchBugReport: vi.fn<() => Promise<unknown>>(),
  fetchDiagnostics: vi.fn<() => Promise<unknown>>(),
  fetchEvents: vi.fn<() => Promise<unknown>>(),
  fetchLogs: vi.fn<() => Promise<unknown>>(),
  fetchSemanticStatus: vi.fn<() => Promise<unknown>>(),
  fetchSyncStatus: vi.fn<() => Promise<unknown>>(),
  installSemanticProfile: vi.fn<(profile: string) => Promise<unknown>>(),
  reindexSemantic: vi.fn<() => Promise<unknown>>(),
  setSemanticEnabled: vi.fn<(enabled: boolean) => Promise<unknown>>(),
  useSemanticProfile: vi.fn<(profile: string) => Promise<unknown>>(),
}));

const accountsApi = vi.hoisted(() => ({
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
}));

vi.mock("./api", () => ({
  backfillSemantic: diagnosticsApi.backfillSemantic,
  fetchAdminStatus: diagnosticsApi.fetchAdminStatus,
  fetchBugReport: diagnosticsApi.fetchBugReport,
  fetchDiagnostics: diagnosticsApi.fetchDiagnostics,
  fetchEvents: diagnosticsApi.fetchEvents,
  fetchLogs: diagnosticsApi.fetchLogs,
  fetchSemanticStatus: diagnosticsApi.fetchSemanticStatus,
  fetchSyncStatus: diagnosticsApi.fetchSyncStatus,
  installSemanticProfile: diagnosticsApi.installSemanticProfile,
  reindexSemantic: diagnosticsApi.reindexSemantic,
  semanticProfiles: ["bge-small-en-v1.5", "multilingual-e5-small", "bge-m3"],
  semanticSnapshot: (response: { status?: unknown; snapshot?: unknown } | undefined) =>
    response?.status ?? response?.snapshot ?? response ?? null,
  setSemanticEnabled: diagnosticsApi.setSemanticEnabled,
  useSemanticProfile: diagnosticsApi.useSemanticProfile,
}));

const router = vi.hoisted(() => ({
  search: {} as { panel?: string },
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
}));

vi.mock("@tanstack/react-router", () => ({
  useSearch: () => router.search,
  useNavigate: () => router.navigate,
}));

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: accountsApi.fetchAccounts,
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

function semanticStatus(enabled = false) {
  return {
    status: {
      enabled,
      active_profile: "bge-small-en-v1.5",
      runtime: { queue_depth: 2, in_flight: 1 },
      profiles: [
        {
          profile: "bge-small-en-v1.5",
          backend: "fastembed",
          model_revision: "v1",
          dimensions: 384,
          status: "ready",
          progress_completed: 12,
          progress_total: 12,
        },
        {
          profile: "multilingual-e5-small",
          backend: "fastembed",
          model_revision: "v1",
          dimensions: 384,
          status: "ready",
          progress_completed: 4,
          progress_total: 12,
        },
      ],
    },
  };
}

beforeEach(() => {
  diagnosticsApi.fetchAdminStatus.mockResolvedValue({ feature_health: {} });
  diagnosticsApi.fetchDiagnostics.mockResolvedValue({ report: {} });
  diagnosticsApi.fetchLogs.mockResolvedValue({ lines: [] });
  diagnosticsApi.fetchEvents.mockResolvedValue({ entries: [] });
  diagnosticsApi.fetchSyncStatus.mockResolvedValue({ healthy: true });
  diagnosticsApi.fetchSemanticStatus.mockResolvedValue(semanticStatus(false));
  diagnosticsApi.backfillSemantic.mockResolvedValue(semanticStatus(true));
  diagnosticsApi.reindexSemantic.mockResolvedValue({ ok: true });
  diagnosticsApi.setSemanticEnabled.mockResolvedValue(semanticStatus(true));
  diagnosticsApi.installSemanticProfile.mockResolvedValue(semanticStatus(true));
  diagnosticsApi.useSemanticProfile.mockResolvedValue(semanticStatus(true));
  accountsApi.fetchAccounts.mockResolvedValue({ accounts: [{ account_id: "account-1" }] });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("DiagnosticValue", () => {
});

describe("DiagnosticsRoute", () => {

  test("choosing a tab puts the panel in the URL", async () => {
    renderWithQueryClient(<DiagnosticsRoute />);

    fireEvent.click(await screen.findByRole("tab", { name: "Events" }));
    expect(router.navigate).toHaveBeenCalledWith({
      to: "/diagnostics",
      search: { panel: "events" },
      replace: true,
    });
  });

});
