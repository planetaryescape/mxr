/*
 * Where the assistant sits: a resizable pane beside the whole app, so it
 * stays put as you move between pages, or an overlay drawer on narrow
 * windows. Closed until opened. The pane and its handle stay mounted and
 * parked while closed (app.css), as the mail list's reader does:
 * registering a panel with the library on open reads layout mid-commit.
 */

import { lazy, Suspense, useState, type ReactNode } from "react";

import { useAssistantUi } from "./assistantStore";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { useSplitPane, type SidePaneSize } from "@/hooks/useSplitPane";

const AssistantSidebar = lazy(async () => ({
  default: (await import("./AssistantSidebar")).AssistantSidebar,
}));

const PANE_SIZE: SidePaneSize = { defaultSize: "400px", minSize: "320px", maxSize: "720px" };

export function AssistantSplit({ children }: { children: ReactNode }) {
  const open = useAssistantUi((s) => s.open);
  const setOpen = useAssistantUi((s) => s.setOpen);
  // Where list and reader stop sharing the screen, the assistant overlays it.
  const drawer = useMediaQuery(SINGLE_PANE_QUERY);
  const beside = open && !drawer;
  const pane = useSplitPane("assistant", PANE_SIZE, { active: beside });
  // Load the sidebar's code on first open, then keep it.
  const [loaded, setLoaded] = useState(open);
  if (open && !loaded) setLoaded(true);

  return (
    <>
      <ResizablePanelGroup className="min-h-0 min-w-0 flex-1" {...pane.groupProps}>
        <ResizablePanel
          id="shell-content"
          {...pane.otherPanelProps}
          className="flex min-h-0 min-w-0"
        >
          {children}
        </ResizablePanel>
        <ResizableHandle
          aria-label="Resize assistant"
          data-parked={!beside || undefined}
          {...pane.handleProps}
        />
        <ResizablePanel
          id="assistant-pane"
          data-parked={!beside || undefined}
          {...pane.sidePanelProps}
          className="flex min-h-0 min-w-0 border-l border-border"
        >
          {loaded && beside ? (
            <Suspense fallback={null}>
              <AssistantSidebar />
            </Suspense>
          ) : null}
        </ResizablePanel>
      </ResizablePanelGroup>
      {open && drawer ? (
        <aside
          className="assistant-drawer"
          aria-label="Assistant"
          onKeyDown={(event) => {
            if (event.key === "Escape" && !(event.target instanceof HTMLTextAreaElement)) {
              setOpen(false);
            }
          }}
        >
          <Suspense fallback={null}>
            <AssistantSidebar />
          </Suspense>
        </aside>
      ) : null}
    </>
  );
}
