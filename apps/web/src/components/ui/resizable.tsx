import { GripVertical } from "lucide-react";
import { useEffect, useRef, type RefObject } from "react";
import * as ResizablePrimitive from "react-resizable-panels";

import { cn } from "@/lib/utils";

const KEEP_PINCH_ZOOM = "[touch-action:pan-y_pinch-zoom]!";

function ResizablePanelGroup({ className, ...props }: ResizablePrimitive.GroupProps) {
  return (
    <ResizablePrimitive.Group
      data-slot="resizable-panel-group"
      // The library forces ew-resize on Chrome; useResizeCursor sets col-resize.
      disableCursor
      className={cn("flex h-full w-full", KEEP_PINCH_ZOOM, className)}
      {...props}
    />
  );
}

function ResizablePanel({ className, ...props }: ResizablePrimitive.PanelProps) {
  return (
    <ResizablePrimitive.Panel
      data-slot="resizable-panel"
      className={cn(KEEP_PINCH_ZOOM, className)}
      {...props}
    />
  );
}

function useResizeCursor(handle: RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const element = handle.current;
    if (!element) return;
    const root = document.documentElement;
    const sync = () => {
      const state = element.getAttribute("data-separator");
      if (state === "hover" || state === "active") root.setAttribute(RESIZING_ATTRIBUTE, "");
      else if (root.hasAttribute(RESIZING_ATTRIBUTE)) root.removeAttribute(RESIZING_ATTRIBUTE);
    };
    const observer = new MutationObserver(sync);
    observer.observe(element, { attributes: true, attributeFilter: ["data-separator"] });
    return () => {
      observer.disconnect();
      root.removeAttribute(RESIZING_ATTRIBUTE);
    };
  }, [handle]);
}

const RESIZING_ATTRIBUTE = "data-pane-resizing";

function ResizableHandle({
  className,
  ...props
}: Omit<ResizablePrimitive.SeparatorProps, "elementRef">) {
  const handle = useRef<HTMLDivElement>(null);
  useResizeCursor(handle);
  return (
    <ResizablePrimitive.Separator
      data-slot="resizable-handle"
      elementRef={handle}
      className={cn(
        "group/handle relative z-10 -mr-px flex w-px cursor-col-resize items-center justify-center bg-border outline-none",
        "after:absolute after:inset-y-0 after:left-1/2 after:w-2 after:-translate-x-1/2",
        "focus-visible:ring-2 focus-visible:ring-ring",
        className,
      )}
      {...props}
    >
      <div
        aria-hidden
        className="flex h-6 w-3 shrink-0 items-center justify-center rounded-sm border border-border bg-background text-muted-foreground opacity-0 transition-opacity duration-fast ease-out group-focus-visible/handle:opacity-100 group-data-[separator=active]/handle:opacity-100 group-data-[separator=hover]/handle:opacity-100"
      >
        <GripVertical className="size-2.5" />
      </div>
    </ResizablePrimitive.Separator>
  );
}

export { ResizableHandle, ResizablePanel, ResizablePanelGroup };
