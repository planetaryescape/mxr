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
});
