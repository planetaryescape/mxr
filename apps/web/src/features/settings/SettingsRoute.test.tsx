/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { LlmSettingsSection } from "./LlmSettingsSection";

const api = vi.hoisted(() => ({
  fetch: vi.fn<(path: string, opts?: unknown) => Promise<unknown>>(),
}));

const accountsApi = vi.hoisted(() => ({
  fetchAccounts: vi.fn<() => Promise<unknown>>(),
  setDefaultAccount: vi.fn<(key: string) => Promise<unknown>>(),
}));

vi.mock("@/api/client", () => ({
  apiFetch: api.fetch,
}));

vi.mock("@/features/accounts/api", () => ({
  fetchAccounts: accountsApi.fetchAccounts,
  setDefaultAccount: accountsApi.setDefaultAccount,
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn<(message: string) => void>(),
    error: vi.fn<(message: string) => void>(),
  },
}));

function renderWithQueryClient(children: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>);
}

describe("LlmSettingsSection", () => {
  beforeEach(() => {
    api.fetch.mockImplementation((path, opts) => {
      if (path === "/api/v1/platform/llm/config" && !opts) {
        return Promise.resolve({
          config: {
            enabled: false,
            base_url: "http://localhost:11434/v1",
            model: "qwen2.5:3b-instruct",
            api_key_env: "",
            context_window: 8192,
            request_timeout_secs: 120,
            allow_cloud_relationship_data: false,
          },
        });
      }
      if (path === "/api/v1/platform/llm/status") {
        return Promise.resolve({
          status: {
            enabled: false,
            provider: "noop",
            model: "noop",
            configured_model: "qwen2.5:3b-instruct",
            base_url: null,
            api_key_env: null,
            api_key_present: false,
            context_window: 0,
            supports_streaming: false,
            request_timeout_secs: 120,
          },
        });
      }
      if (path === "/api/v1/platform/llm/config" && opts) {
        return Promise.resolve({
          config: {
            enabled: true,
            base_url: "https://api.openai.com/v1",
            model: "gpt-5-mini",
            api_key_env: "OPENAI_API_KEY",
            context_window: 16384,
            request_timeout_secs: 45,
            allow_cloud_relationship_data: true,
          },
        });
      }
      throw new Error(`unexpected request: ${path}`);
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("loads the daemon LLM config and saves only an API-key environment variable", async () => {
    renderWithQueryClient(<LlmSettingsSection />);

    expect(await screen.findByDisplayValue("http://localhost:11434/v1")).toBeVisible();
    expect(screen.getByText(/provider: noop/i)).toBeVisible();

    fireEvent.click(screen.getByRole("switch", { name: /enable llm features/i }));
    fireEvent.change(screen.getByLabelText(/^base url$/i), {
      target: { value: "https://api.openai.com/v1" },
    });
    fireEvent.change(screen.getByLabelText(/^model$/i), {
      target: { value: "gpt-5-mini" },
    });
    fireEvent.change(screen.getByLabelText(/api key environment variable/i), {
      target: { value: "OPENAI_API_KEY" },
    });
    fireEvent.change(screen.getByLabelText(/context window/i), {
      target: { value: "16384" },
    });
    fireEvent.change(screen.getByLabelText(/request timeout/i), {
      target: { value: "45" },
    });
    fireEvent.click(screen.getByRole("switch", { name: /allow relationship data/i }));
    fireEvent.click(screen.getByRole("button", { name: /save llm config/i }));

    await waitFor(() => {
      expect(api.fetch).toHaveBeenCalledWith("/api/v1/platform/llm/config", {
        method: "POST",
        body: {
          enabled: true,
          base_url: "https://api.openai.com/v1",
          model: "gpt-5-mini",
          api_key_env: "OPENAI_API_KEY",
          context_window: 16384,
          request_timeout_secs: 45,
          allow_cloud_relationship_data: true,
          models: [],
          overrides: expect.any(Object),
        },
      });
    });
    expect(screen.queryByLabelText(/^api key$/i)).not.toBeInTheDocument();
  });
});
