import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { resetRowGistsForTest } from "@/features/gists/rowGists";

import type { UpdateLine } from "./api";
import { lineFixture } from "./testing";
import { UpdateLineRow } from "./UpdateLineRow";

const apiFetch = vi.fn<typeof import("@/api/client").apiFetch>();
vi.mock("@/api/client", () => ({
  apiFetch: (...args: Parameters<typeof apiFetch>) => apiFetch(...args),
}));

beforeEach(() => {
  apiFetch.mockReset();
  apiFetch.mockResolvedValue({ thread: {}, messages: [], bodies: [] });
  resetRowGistsForTest();
});

function makeHandlers() {
  return {
    onSelect: vi.fn<(line: UpdateLine) => void>(),
    onLetGo: vi.fn<(line: UpdateLine) => void>(),
    onNeedsMe: vi.fn<(line: UpdateLine) => void>(),
    onTune: vi.fn<(line: UpdateLine) => void>(),
    onOpenEmail: vi.fn<(line: UpdateLine) => void>(),
    onTuneTo: vi.fn<(line: UpdateLine, setting: "muted" | "changes_only") => void>(),
    onSweep: vi.fn<(line: UpdateLine) => void>(),
    onPin: vi.fn<(line: UpdateLine) => void>(),
    onTogglePeek: vi.fn<(line: UpdateLine) => void>(),
  };
}

function renderLine(line: UpdateLine, options: { focused?: boolean; peeked?: boolean } = {}) {
  const h = makeHandlers();
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <ul>
        <UpdateLineRow
          line={line}
          index={0}
          lit={options.focused ?? true}
          via="key"
          peeked={options.peeked ?? false}
          {...h}
        />
      </ul>
    </QueryClientProvider>,
  );
  return { handlers: h, row: screen.getByTestId("update-line") };
}

describe("UpdateLineRow at rest", () => {
  test("a row is its source, its fact and one trailing value, with no buttons in the way", () => {
    const { row } = renderLine(lineFixture(), { focused: false });
    const view = within(row);
    expect(view.getByTestId("update-source")).toHaveTextContent("Strava");
    expect(view.getByTestId("update-fact")).toHaveTextContent(
      "Your week in running: 21.3 km over 3 runs",
    );
    expect(view.getByTestId("update-fact")).toHaveAttribute("title", lineFixture().fact);
    expect(view.getByTestId("update-delta")).toHaveTextContent("up 12% on last week");
    expect(view.getByTestId("update-delta")).toHaveAttribute(
      "title",
      "Computed from 21.3 km and 19.0 km",
    );
    expect(row).not.toHaveAttribute("data-lit");
    expect(view.getByRole("button", { name: "More actions for Strava" })).toBeInTheDocument();
    expect(view.queryByText("Why this update?")).toBeNull();
    expect(view.queryByRole("link")).toBeNull();
    expect(view.queryByTestId("update-peek")).toBeNull();
  });
});

describe("UpdateLineRow's peek", () => {
  test("peeking acknowledges this row, while keyboard focus alone does not", async () => {
    const line = lineFixture({ unread: true, seen_revision: "exact-revision" });
    renderLine(line, { peeked: false });
    expect(apiFetch).not.toHaveBeenCalledWith("/api/v1/mail/rows/seen", expect.anything());
    cleanup();
    renderLine(line, { peeked: true });
    await waitFor(() =>
      expect(apiFetch).toHaveBeenCalledWith("/api/v1/mail/rows/seen", {
        method: "POST",
        body: {
          mode: "updates",
          account_id: line.account_id,
          row_id: line.id,
          revision: "exact-revision",
        },
      }),
    );
  });
});

describe("UpdateLineRow with a one-time code", () => {
  test("the code shows on the line and a click copies it without opening the peek", async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>().mockResolvedValue();
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { row, handlers } = renderLine(
      lineFixture({ source_name: "Acme", fact: "MFA Login Authentication", code: "882308" }),
    );
    const code = within(row).getByTestId("update-code");
    expect(code).toHaveTextContent("882308");
    fireEvent.click(code);
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("882308"));
    expect(handlers.onTogglePeek).not.toHaveBeenCalled();
    expect(within(row).getAllByRole("button", { name: /copy code/i })).toHaveLength(2);
  });
});
