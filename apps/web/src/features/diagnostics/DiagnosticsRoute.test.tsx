/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

vi.mock("./api", () => ({
  fetchAdminStatus: vi.fn<() => Promise<unknown>>(),
  fetchBugReport: vi.fn<() => Promise<unknown>>(),
  fetchDiagnostics: vi.fn<() => Promise<unknown>>(),
  fetchEvents: vi.fn<() => Promise<unknown>>(),
  fetchLogs: vi.fn<() => Promise<unknown>>(),
  fetchSemanticStatus: vi.fn<() => Promise<unknown>>(),
  fetchSyncStatus: vi.fn<() => Promise<unknown>>(),
}));

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
}));

import { DiagnosticValue } from "./DiagnosticsRoute";

describe("DiagnosticValue", () => {
  test("renders object diagnostics as readable key-value rows", () => {
    render(<DiagnosticValue value={{ ok: true, latency_ms: 12 }} />);

    expect(screen.getByText("ok")).toBeVisible();
    expect(screen.getByText("true")).toBeVisible();
    expect(screen.getByText("latency_ms")).toBeVisible();
    expect(screen.getByText("12")).toBeVisible();
    expect(screen.queryByText(/"ok"/)).not.toBeInTheDocument();
  });
});
