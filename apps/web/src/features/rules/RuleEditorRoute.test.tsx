/* @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RuleEditorRoute } from "./RuleEditorRoute";

vi.mock("@tanstack/react-router", () => ({
  useParams: () => ({ id: "new" }),
  useNavigate: () => async () => {},
}));

function mount(categoryError = false, multipleCategories = false) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("categories") && url.includes("rules")) {
        return categoryError
          ? new Response(JSON.stringify({ error: "Category preview failed" }), { status: 400 })
          : new Response(
              JSON.stringify({
                applied: false,
                rule_id: "rule",
                token: "token",
                complete: true,
                delete: false,
                unavailable: 0,
                notice: "complete",
                matches: multipleCategories
                  ? ["Travel", "Home"].map((category_name) => ({
                      message_id: "message",
                      account_id: "account",
                      category_id: category_name.toLowerCase(),
                      category_name,
                      from: "airline@example.com",
                      subject: "Flight ticket",
                      before: false,
                      after: true,
                      reason: "Added by Flights",
                      manual_decision: null,
                      winner: "rule",
                    }))
                  : [],
              }),
            );
      }
      if (url.includes("/categories"))
        return new Response(
          JSON.stringify({
            categories: [{ id: "travel", name: "Travel", unread_count: 0, total_count: 0 }],
          }),
        );
      if (url.includes("/search"))
        return new Response(
          JSON.stringify({
            groups: [{ rows: [{ id: "message", sender: "Airline", subject: "Flight ticket" }] }],
          }),
        );
      if (url.includes("accounts")) return new Response(JSON.stringify({ accounts: [] }));
      return new Response(JSON.stringify({}));
    }),
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <RuleEditorRoute />
    </QueryClientProvider>,
  );
  fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Flights" } });
  fireEvent.change(screen.getByLabelText("When a message matches"), {
    target: { value: "subject:flight" },
  });
}

async function settlePreview() {
  for (let elapsed = 0; elapsed < 1_000; elapsed += 50)
    // oxlint-disable-next-line no-await-in-loop -- each step needs the previous step's commit
    await act(() => vi.advanceTimersByTimeAsync(50));
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("rule preview confirmation", () => {
  it("lets an ordinary rule open Apply and dismiss the confirmation", async () => {
    mount();
    fireEvent.change(screen.getByLabelText("Do"), { target: { value: "archive" } });
    await settlePreview();
    const apply = screen.getByRole("button", { name: "Apply to 1 message" });
    expect(apply).toBeEnabled();
    fireEvent.click(apply);
    expect(screen.getByRole("button", { name: "Apply now" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await settlePreview();
    expect(screen.queryByRole("button", { name: "Apply now" })).not.toBeInTheDocument();
  });
});
