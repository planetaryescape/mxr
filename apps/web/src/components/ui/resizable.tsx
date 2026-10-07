import { GripVertical } from "lucide-react";
import * as ResizablePrimitive from "react-resizable-panels";

import { cn } from "@/lib/utils";

/*
 * shadcn's Resizable over react-resizable-panels v4 (Group, Panel,
 * Separator). The handle is a 1px rule with an 8px hit strip and a grip
 * that shows on hover, drag and keyboard focus. The library owns pointer,
 * keyboard (arrows, Home, End, Enter) and double-click-to-reset; the
 * col-resize cursor comes from app.css, keyed on the handle's state.
 */

/*
 * The library sets `touch-action: pan-y` inline on groups and panels, and
 * touch-action intersects down the tree, so every split would switch off
 * pinch-zoom for everything inside it. Handles keep `none`, which is what
 * a touch drag needs. Horizontal splits only: a vertical one needs pan-x.
 */
const KEEP_PINCH_ZOOM = "[touch-action:pan-y_pinch-zoom]!";

function ResizablePanelGroup({ className, ...props }: ResizablePrimitive.GroupProps) {
  return (
    <ResizablePrimitive.Group
      data-slot="resizable-panel-group"
      // The library forces ew-resize on Chrome; app.css sets col-resize.
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

function ResizableHandle({ className, ...props }: ResizablePrimitive.SeparatorProps) {
  return (
    <ResizablePrimitive.Separator
      data-slot="resizable-handle"
      className={cn(
        "group/handle relative z-10 flex w-px cursor-col-resize items-center justify-center bg-border outline-none",
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
