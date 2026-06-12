/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, test, vi } from "vitest";

import { RecipientField } from "./RecipientField";

const api = vi.hoisted(() => ({
  fetchContactsAutocomplete: vi.fn<(query: string) => Promise<unknown[]>>(),
}));

vi.mock("./api", () => ({
  fetchContactsAutocomplete: api.fetchContactsAutocomplete,
}));

function Harness({ initial = "" }: { initial?: string }) {
  const [value, setValue] = useState(initial);
  return <RecipientField label="To" value={value} onChange={setValue} />;
}

function renderField(initial = "") {
  api.fetchContactsAutocomplete.mockResolvedValue([]);
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <Harness initial={initial} />
    </QueryClientProvider>,
  );
}

describe("RecipientField", () => {
  test("flags an unparseable chip as invalid on commit", () => {
    renderField();

    const input = screen.getByRole("combobox", { name: "To" });
    fireEvent.change(input, { target: { value: "not-an-email" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(screen.getByTitle("Invalid address: not-an-email")).toBeVisible();
  });

  test("does not flag a valid address chip", () => {
    renderField("alpha@example.com");

    expect(screen.getByText("alpha@example.com")).toBeVisible();
    expect(screen.queryByTitle(/invalid address/i)).not.toBeInTheDocument();
  });

  test("parses pasted RFC 5322 address lists into normalized chips", () => {
    renderField();

    const input = screen.getByRole("combobox", { name: "To" });
    fireEvent.paste(input, {
      clipboardData: {
        getData: () => '"Jane Doe" <jane@example.com>, bob@example.com',
      },
    });

    expect(screen.getByText("Jane Doe")).toBeVisible();
    expect(screen.getByText("bob@example.com")).toBeVisible();
    expect(screen.queryByTitle(/invalid address/i)).not.toBeInTheDocument();
  });

  test("exposes combobox semantics with an active descendant while open", async () => {
    renderField();
    api.fetchContactsAutocomplete.mockResolvedValue([
      { email: "suggested@example.com", display_name: "Suggested Person" },
    ]);

    const input = screen.getByRole("combobox", { name: "To" });
    expect(input).toHaveAttribute("aria-expanded", "false");

    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "sugg" } });

    const option = await screen.findByRole("option", { name: /suggested person/i });
    expect(input).toHaveAttribute("aria-expanded", "true");
    expect(input).toHaveAttribute("aria-activedescendant", option.id);
  });
});
