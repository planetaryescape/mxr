/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";

import { AskArchivePanel } from "./AskArchivePanel";
import { WhoisPanel } from "./WhoisPanel";

const client = vi.hoisted(() => ({
  apiFetch: vi.fn<(path: string, opts?: unknown) => Promise<unknown>>(),
}));
const router = vi.hoisted(() => ({ navigate: vi.fn<(options: unknown) => void>() }));

vi.mock("@/api/client", () => ({ apiFetch: client.apiFetch }));
vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => router.navigate,
  Link: ({ children }: { children: ReactNode }) => <a href="/settings/llm">{children}</a>,
}));

function renderWithClient(node: ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(<QueryClientProvider client={queryClient}>{node}</QueryClientProvider>);
}

function respond(routes: Record<string, unknown>) {
  client.apiFetch.mockImplementation((path) => {
    const key = Object.keys(routes).find((prefix) => path.startsWith(prefix));
    return key ? Promise.resolve(routes[key]) : Promise.reject(new Error(`unexpected ${path}`));
  });
}

afterEach(() => vi.clearAllMocks());

describe("WhoisPanel", () => {
  test("shows the summary and points to LLM setup when no model is configured", async () => {
    respond({
      "/api/v1/platform/llm/status": { status: { enabled: false } },
      "/api/v1/mail/whois": {
        kind: "EntityExplanation",
        entity: {
          canonical_name: "cal@signal.example",
          kind: "person",
          summary: "Cal Brooks, 10 inbound, 0 outbound.",
          first_seen_at: "2026-09-26T02:33:46Z",
          last_seen_at: "2026-09-26T19:32:46Z",
          topics: ["release"],
          citations: [{ msg_id: "m1", quote: "The build is red again." }],
          candidates: [],
        },
      },
    });
    renderWithClient(<WhoisPanel entity="cal@signal.example" accountId="acct-1" />);

    expect(await screen.findByText("Cal Brooks, 10 inbound, 0 outbound.")).toBeVisible();
    expect(client.apiFetch).toHaveBeenCalledWith(
      "/api/v1/mail/whois?query=cal%40signal.example&account=acct-1",
    );
    expect(screen.getByText("release")).toBeVisible();
    expect(screen.getByText("The build is red again.")).toBeVisible();
    expect(await screen.findByRole("link", { name: /set up a language model/i })).toBeVisible();
  });
});

describe("AskArchivePanel", () => {
  test("asks, shows the answer and opens a cited thread", async () => {
    respond({
      "/api/v1/platform/llm/status": { status: { enabled: true } },
      "/api/v1/mail/archive-ask": {
        kind: "ArchiveAnswer",
        answer: {
          text: "Launch moved to 3 October.",
          citations: [
            {
              message_id: "m1",
              thread_id: "t1",
              subject: "Launch date",
              date: "2026-09-20T10:00:00Z",
              quote: "Let's go with the 3rd.",
            },
          ],
          retrieval: { requested_mode: "hybrid", executed_mode: "hybrid", candidate_count: 1 },
        },
      },
    });
    renderWithClient(<AskArchivePanel />);

    fireEvent.change(screen.getByLabelText("Question for your archive"), {
      target: { value: "When is launch?" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Ask" }));

    expect(await screen.findByText("Launch moved to 3 October.")).toBeVisible();
    expect(client.apiFetch).toHaveBeenCalledWith("/api/v1/mail/archive-ask", {
      method: "POST",
      body: { question: "When is launch?", limit: undefined },
    });
    expect(screen.getByText("1 message considered, hybrid search", { exact: false })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /launch date/i }));
    await waitFor(() =>
      expect(router.navigate).toHaveBeenCalledWith({
        to: "/m/$mailbox/$threadId",
        params: { mailbox: "archive", threadId: "t1" },
      }),
    );
  });
});
