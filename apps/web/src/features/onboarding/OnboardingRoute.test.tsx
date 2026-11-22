/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { OnboardingRoute } from "./OnboardingRoute";
import type { AuthSession } from "@/features/accounts/api";

const router = vi.hoisted(() => ({
  navigate: vi.fn<(options: unknown) => Promise<void>>(),
}));

const api = vi.hoisted(() => ({
  startAuthSession:
    vi.fn<(account: unknown, reauth?: boolean) => Promise<{ session: AuthSession }>>(),
  fetchAuthSession: vi.fn<(id: string) => Promise<{ session: AuthSession }>>(),
  completeAuthSession: vi.fn<(id: string) => Promise<{ session: AuthSession }>>(),
  cancelAuthSession: vi.fn<(id: string) => Promise<unknown>>(),
  testAccount: vi.fn<(account: unknown) => Promise<unknown>>(),
  upsertAccount: vi.fn<(account: unknown) => Promise<unknown>>(),
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
}));

vi.mock("@/features/accounts/api", () => ({
  startAuthSession: api.startAuthSession,
  fetchAuthSession: api.fetchAuthSession,
  completeAuthSession: api.completeAuthSession,
  cancelAuthSession: api.cancelAuthSession,
  testAccount: api.testAccount,
  upsertAccount: api.upsertAccount,
  gmailAccountConfig: (email: string) => ({ key: `gmail-${email}`, sync: { type: "gmail" } }),
  outlookAccountConfig: (email: string) => ({
    key: `outlook-${email}`,
    sync: { type: "outlook_personal" },
  }),
  imapAccountConfig: (input: { email: string }) => ({ key: `imap-${input.email}` }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(message?: unknown) => void>(),
    error: vi.fn<(message?: unknown) => void>(),
  },
}));

function renderOnboarding(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

function session(overrides: Partial<AuthSession>): AuthSession {
  return { session_id: "sess-1", state: "waiting_for_user", ...overrides };
}

async function reachAuthStep(provider: "gmail" | "outlook", startSession: AuthSession) {
  api.startAuthSession.mockResolvedValue({ session: startSession });
  api.fetchAuthSession.mockResolvedValue({ session: startSession });
  renderOnboarding(<OnboardingRoute />);
  fireEvent.click(screen.getByRole("button", { name: /connect first account/i }));
  if (provider === "outlook") {
    fireEvent.click(screen.getByRole("radio", { name: /outlook/i }));
  }
  fireEvent.change(screen.getByPlaceholderText(/you@example\.com/i), {
    target: { value: `user@${provider}.com` },
  });
  fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
  await waitFor(() => expect(api.startAuthSession).toHaveBeenCalled());
}

describe("OnboardingRoute auth step", () => {
  beforeEach(() => {
    vi.stubGlobal("open", vi.fn<(...args: unknown[]) => unknown>());
  });
  afterEach(() => {
    vi.clearAllMocks();
    vi.unstubAllGlobals();
  });

  test("completing an authorized session advances and saves the account", async () => {
    api.completeAuthSession.mockResolvedValue({ session: session({ state: "authorized" }) });
    await reachAuthStep(
      "gmail",
      session({ state: "authorized", auth_url: "https://accounts.google.com/o/oauth2/v2/auth" }),
    );

    const complete = await screen.findByRole("button", { name: /^complete$/i });
    expect(complete).toBeEnabled();
    fireEvent.click(complete);

    await waitFor(() => expect(api.completeAuthSession).toHaveBeenCalledWith("sess-1"));
    expect(await screen.findByRole("button", { name: /open now/i })).toBeVisible();
    expect(screen.getByText(/waiting for the first sync to start/i)).toBeVisible();
    expect(screen.queryByText(/^ready$/i)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /open now/i }));
    await waitFor(() => expect(router.navigate).toHaveBeenCalledWith({ to: "/now" }));
  });

  test("Cancel cancels the daemon session and returns to provider choice", async () => {
    api.cancelAuthSession.mockResolvedValue(undefined);
    await reachAuthStep(
      "gmail",
      session({ auth_url: "https://accounts.google.com/o/oauth2/v2/auth" }),
    );

    fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));

    await waitFor(() => expect(api.cancelAuthSession).toHaveBeenCalledWith("sess-1"));
    expect(await screen.findByRole("heading", { name: /choose a provider/i })).toBeVisible();
  });
});
