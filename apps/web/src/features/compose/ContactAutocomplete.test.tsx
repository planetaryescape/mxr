/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { ContactAutocomplete } from "./ContactAutocomplete";

const api = vi.hoisted(() => ({
  fetchContactsAutocomplete: vi.fn<(q: string, limit?: number) => Promise<unknown>>(),
}));

vi.mock("./api", () => ({
  fetchContactsAutocomplete: api.fetchContactsAutocomplete,
}));

function wrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

describe("ContactAutocomplete", () => {
  let client: QueryClient;

  beforeEach(() => {
    client = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  test("shows suggestions after debounce and commits on Enter (highlight=0)", async () => {
    api.fetchContactsAutocomplete.mockResolvedValue([
      { email: "alice@example.com", display_name: "Alice" },
      { email: "alex@example.com", display_name: "Alex" },
    ]);
    const onSelect = vi.fn<(email: string) => void>();
    const Wrap = wrapper(client);

    render(
      <Wrap>
        <ContactAutocomplete
          label="To"
          value="al"
          onChange={() => {}}
          onSelect={onSelect}
        />
      </Wrap>,
    );

    const input = screen.getByRole("combobox", { name: "To" });
    fireEvent.focus(input);

    await waitFor(
      () => {
        const calls = api.fetchContactsAutocomplete.mock.calls;
        expect(calls.length).toBeGreaterThan(0);
        expect(calls[calls.length - 1]?.[0]).toBe("al");
      },
      { timeout: 1000 },
    );
    const items = await screen.findAllByRole("option");
    expect(items.length).toBe(2);

    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith("alice@example.com");
  });

  test("ArrowDown highlights next suggestion before commit", async () => {
    api.fetchContactsAutocomplete.mockResolvedValue([
      { email: "alice@example.com", display_name: "Alice" },
      { email: "alex@example.com", display_name: "Alex" },
    ]);
    const onSelect = vi.fn<(email: string) => void>();
    const Wrap = wrapper(client);

    render(
      <Wrap>
        <ContactAutocomplete
          label="To"
          value="al"
          onChange={() => {}}
          onSelect={onSelect}
        />
      </Wrap>,
    );
    const input = screen.getByRole("combobox", { name: "To" });
    fireEvent.focus(input);
    await screen.findAllByRole("option");

    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith("alex@example.com");
  });

  test("does not query when input is empty", async () => {
    const Wrap = wrapper(client);
    render(
      <Wrap>
        <ContactAutocomplete label="To" value="" onChange={() => {}} onSelect={() => {}} />
      </Wrap>,
    );
    // Wait long enough to cover the 200ms debounce.
    await new Promise((r) => setTimeout(r, 350));
    expect(api.fetchContactsAutocomplete).not.toHaveBeenCalled();
  });
});
