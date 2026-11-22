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
  const drawer = useMediaQuery(SINGLE_PANE_QUERY);
  const beside = open && !drawer;
  const pane = useSplitPane("assistant", PANE_SIZE, { active: beside });
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
