import { cleanup, render, screen } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { ReactNode } from "react";

import { ArchiveSkeleton } from "./ArchiveSkeleton";
import { CARD_PANE_SIZE } from "./archiveLayout";

interface LayoutState {
  single: boolean;
  threadId?: string;
}
interface PanelProps {
  children: ReactNode;
  id?: string;
  defaultSize?: string;
  minSize?: string;
  maxSize?: string;
}
const layout = vi.hoisted((): LayoutState => ({ single: false }));
const splitPane = vi.hoisted(() =>
  vi.fn<(...args: unknown[]) => object>(() => ({
    groupProps: {},
    otherPanelProps: {},
    sidePanelProps: { defaultSize: "420px", minSize: "320px", maxSize: "560px" },
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
  ResizablePanel: ({ children, id, defaultSize, minSize, maxSize }: PanelProps) => (
    <div id={id} data-default-size={defaultSize} data-min-size={minSize} data-max-size={maxSize}>
      {children}
    </div>
  ),
}));

beforeEach(() => {
  layout.single = false;
  layout.threadId = undefined;
  splitPane.mockClear();
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
