import { cleanup, render, screen } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { ReactNode } from "react";

import { ArchiveSkeleton } from "./ArchiveSkeleton";
import { CARD_PANE_SIZE } from "./archiveLayout";

const layout = vi.hoisted((): { single: boolean; threadId?: string } => ({ single: false }));
const splitPane = vi.hoisted(() =>
  vi.fn<(...args: unknown[]) => object>(() => ({
    groupProps: {},
    otherPanelProps: {},
    sidePanelProps: {},
  })),
);
vi.mock("@tanstack/react-router", () => ({ useParams: () => ({ threadId: layout.threadId }) }));
vi.mock("@/hooks/useMediaQuery", () => ({
  SINGLE_PANE_QUERY: "single-pane",
  useMediaQuery: () => layout.single,
}));
vi.mock("@/hooks/useSplitPane", () => ({ useSplitPane: splitPane }));
vi.mock("@/components/ui/resizable", () => ({
  ResizablePanelGroup: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ResizablePanel: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

beforeEach(() => {
  layout.single = false;
  layout.threadId = undefined;
  splitPane.mockClear();
});

test("Archive reserves its question, coming-up list, filters, amount rows and card", () => {
  const { container } = render(<ArchiveSkeleton shown />);
  expect(screen.getByRole("status", { name: "Loading Archive" })).toHaveAttribute(
    "aria-busy",
    "true",
  );
  expect(screen.getByTestId("archive-search-placeholder")).toBeInTheDocument();
  expect(screen.getByTestId("archive-filters-placeholder")).toBeInTheDocument();
  expect(screen.getByTestId("archive-card-placeholder")).toBeInTheDocument();
  expect(container.querySelector("h2")).toHaveTextContent("Coming up");
  expect(container.querySelector('[data-skeleton-row="amount"]')).toBeInTheDocument();
  // Reuse the real ledger's preference key and constraints, not a second arbitrary split.
  expect(splitPane).toHaveBeenCalledWith("archive-card", CARD_PANE_SIZE, { active: true });
  expect(container.querySelector("button, a, input, [tabindex]")).toBeNull();
});

test("a phone or an already-open email does not reserve a hidden record card", () => {
  layout.single = true;
  render(<ArchiveSkeleton shown />);
  expect(screen.queryByTestId("archive-card-placeholder")).toBeNull();
  expect(screen.getByTestId("archive-search-placeholder")).toBeInTheDocument();
  cleanup();
  layout.single = false;
  layout.threadId = "open-email";
  render(<ArchiveSkeleton shown />);
  expect(screen.queryByTestId("archive-card-placeholder")).toBeNull();
  expect(splitPane).toHaveBeenLastCalledWith("archive-card", CARD_PANE_SIZE, { active: false });
});

test("quiet Archive reserves the same layout without showing bars", () => {
  const { container } = render(<ArchiveSkeleton shown={false} />);
  expect(screen.queryByTestId("page-skeleton")).toBeNull();
  expect(container.querySelector('[aria-hidden="true"]')).toHaveClass("invisible");
});
