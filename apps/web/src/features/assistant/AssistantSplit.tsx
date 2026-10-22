/*
 * Where the assistant sits: a resizable pane beside the whole app, so it
 * stays put as you move between pages, or an overlay drawer on narrow
 * windows. Closed until opened. The pane and its handle stay mounted and
 * parked while closed (app.css), as the mail list's reader does:
 * registering a panel with the library on open reads layout mid-commit.
 */

import { lazy, Suspense, useState, type ReactNode } from "react";

import { cn } from "@/lib/utils";

import { useAssistantUi, type OpenVia } from "./assistantStore";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { useSplitPane, type SidePaneSize } from "@/hooks/useSplitPane";

const AssistantSidebar = lazy(async () => ({
  default: (await import("./AssistantSidebar")).AssistantSidebar,
}));

const PANE_SIZE: SidePaneSize = {
  defaultSize: "400px",
  minSize: "320px",
  maxSize: "720px",
};

/** Slides in for a pointer only; a key opens it at once (D134). Reports when the slide is over. */
function Arrival({
  via,
  onSettled,
  className,
  children,
}: {
  via: OpenVia | null;
  onSettled: () => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex min-h-0 min-w-0 flex-1",
        via === "pointer" && "assistant-arrive",
        className,
      )}
      // Children animate too (the live dot); only this wrapper's own slide ends it.
      onAnimationEnd={(event) => {
        if (event.target === event.currentTarget) onSettled();
      }}
    >
      {children}
    </div>
  );
}

export function AssistantSplit({ children }: { children: ReactNode }) {
  const open = useAssistantUi((s) => s.open);
  const setOpen = useAssistantUi((s) => s.setOpen);
  const via = useAssistantUi((s) => s.via);
  const settle = useAssistantUi((s) => s.settle);
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
            <Arrival via={via} onSettled={settle}>
              <Suspense fallback={null}>
                <AssistantSidebar />
              </Suspense>
            </Arrival>
          ) : null}
        </ResizablePanel>
      </ResizablePanelGroup>
      {open && drawer ? (
        <aside
          className={cn("assistant-drawer", via === "pointer" && "assistant-arrive")}
          onAnimationEnd={(event) => {
            if (event.target === event.currentTarget) settle();
          }}
          aria-label="Assistant"
          onKeyDown={(event) => {
            if (
              event.key === "Escape" &&
              !event.defaultPrevented &&
              !(event.target instanceof HTMLTextAreaElement)
            ) {
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
