/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { ComposeLauncher } from "./ComposeLauncher";
import { useComposeUi } from "./composeUiStore";
import { useModals } from "@/state/modalStore";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
}));

const analytics = vi.hoisted(() => ({
  fetchContactAsymmetry: vi.fn<() => Promise<unknown>>(),
  fetchContactDecay: vi.fn<() => Promise<unknown>>(),
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
}));

vi.mock("@/features/analytics/api", () => ({
  fetchContactAsymmetry: analytics.fetchContactAsymmetry,
  fetchContactDecay: analytics.fetchContactDecay,
}));

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("ComposeLauncher", () => {
  beforeEach(() => {
    useModals.setState({ composeLauncherOpen: true });
    analytics.fetchContactAsymmetry.mockResolvedValue({
      rows: [
        {
          email: "sender@example.com",
          display_name: "Sender Example",
          inbound: 9,
          outbound: 3,
        },
      ],
    });
    analytics.fetchContactDecay.mockResolvedValue({ rows: [] });
  });

  afterEach(() => {
    vi.clearAllMocks();
    useModals.setState({ composeLauncherOpen: false });
    useComposeUi.setState({ intent: null });
  });

  test("hands To and Subject to the compose surface without leaving the page", async () => {
    renderWithQueryClient(<ComposeLauncher />);

    const to = await screen.findByRole("textbox", { name: "Recipients" });
    fireEvent.change(to, { target: { value: "qa@example.com" } });
    fireEvent.keyDown(to, { key: "Enter" });
    const subject = await screen.findByRole("textbox", { name: "Subject" });
    fireEvent.change(subject, { target: { value: "Launch notes" } });
    fireEvent.keyDown(subject, { key: "Enter" });

    expect(useComposeUi.getState().intent).toMatchObject({
      kind: "new",
      prefillTo: "qa@example.com",
      prefillSubject: "Launch notes",
    });
    expect(useModals.getState().composeLauncherOpen).toBe(false);
    expect(router.navigate).not.toHaveBeenCalled();
  });

  test("suggests recipients and accepts the ghost completion with tab", async () => {
    renderWithQueryClient(<ComposeLauncher />);

    const input = await screen.findByRole("textbox", { name: "Recipients" });
    fireEvent.change(input, { target: { value: "sen" } });

    expect(await screen.findByText("Sender Example")).toBeVisible();

    fireEvent.keyDown(input, { key: "Tab" });

    expect(input).toHaveValue("sender@example.com");
  });
});
