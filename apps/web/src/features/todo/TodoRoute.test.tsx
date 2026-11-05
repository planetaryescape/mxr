import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import { useState, type ReactNode } from "react";
import { expect, test, vi } from "vitest";

import { runCommand } from "@/lib/keys/controllers";

import { runwayFixture } from "./testing";
import { TodoRoute } from "./TodoRoute";

const closeHint = vi.hoisted(() => vi.fn<() => void>());

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  fetchRunway: async () => runwayFixture(),
}));
vi.mock("@/features/mailbox/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/mailbox/api")>()),
  fetchThread: async () => ({ messages: [] }),
}));
vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: ReactNode }) => children,
  useNavigate: () => vi.fn<() => void>(),
  useParams: () => ({}),
}));
vi.mock("@/features/places/PlaceLayout", () => ({
  PlaceLayout: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/features/modes/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/modes/api")>()),
  useModeGuide: () => ({ data: undefined }),
}));
vi.mock("@/features/modes/membership", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/features/modes/membership")>()),
  useThreadModesMap: () => ({ data: undefined }),
}));
vi.mock("@/features/modes/rowSeen", () => ({ useMarkRowSeen: () => () => undefined }));
vi.mock("@/features/hints/useHint", () => ({
  useHint: () => ({ hint: undefined, dismiss: () => undefined }),
  useActiveHintDismiss: () => closeHint,
}));
// The open peek is the `source` search param; keep it in component state here.
vi.mock("@/hooks/useSearchParam", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/hooks/useSearchParam")>()),
  useSearchParam: () => useState<string | null>(null),
}));

test("Space peeks under the lit to-do, and Esc closes the peek before any hint", async () => {
  // jsdom has no layout; the list scrolls the lit row into view.
  Element.prototype.scrollIntoView = vi.fn<() => void>();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <TodoRoute />
    </QueryClientProvider>,
  );
  await screen.findByText("Pay council tax", {}, { timeout: 3000 });
  expect(screen.queryByTestId("todo-source")).toBeNull();

  act(() => void runCommand("todo", "details"));
  expect(screen.getByTestId("todo-why")).toHaveTextContent("payment due 9 October");

  act(() => void runCommand("todo", "closeHint"));
  expect(screen.queryByTestId("todo-source")).toBeNull();
  expect(closeHint).not.toHaveBeenCalled();

  act(() => void runCommand("todo", "closeHint"));
  expect(closeHint).toHaveBeenCalledOnce();
});
