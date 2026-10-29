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

/**
 * The editor waits 350 ms after the last edit before it previews, then holds
 * any skeleton that appeared for 400 ms. On real timers that wait came out of
 * `findBy*`'s 1 s budget, leaving too little for the fetch, the renders and the
 * role queries when a full-suite run starved the worker. Fake timers make the
 * wait free, so the result no longer depends on machine load. The clock moves
 * in short steps, each in its own `act`, because one long step left the
 * preview unrendered: its result, and then the skeleton's release, each landed
 * only on a later step.
 */
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
  // Before setup.ts's afterEach, which waits on a real 0 ms timer.
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
  it("keeps Cancel usable when a category preview fails", async () => {
    mount(true);
    fireEvent.change(screen.getByLabelText("Do"), { target: { value: "add-category:travel" } });
    await settlePreview();
    expect(screen.getByText("Dry run failed")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Save rule" }));
    expect(screen.getByRole("button", { name: "Apply now" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await settlePreview();
    expect(screen.queryByRole("button", { name: "Apply now" })).not.toBeInTheDocument();
  });
  it("shows two category effects as one message with distinct category rows", async () => {
    const errors = vi.spyOn(console, "error");
    mount(false, true);
    fireEvent.change(screen.getByLabelText("Do"), {
      target: { value: "add-category:travel,add-category:home" },
    });
    await settlePreview();
    expect(screen.getByText("Travel: Flight ticket")).toBeInTheDocument();
    expect(screen.getByText("Home: Flight ticket")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Apply to 1 message" }));
    expect(screen.getByRole("heading", { name: "Apply to 1 message?" })).toBeInTheDocument();
    expect(errors.mock.calls.some(([message]) => String(message).includes("same key"))).toBe(false);
    errors.mockRestore();
  });
});
