import { useCallback, useLayoutEffect, useRef } from "react";
import { usePanelRef, type Layout, type PanelSize } from "react-resizable-panels";

import { useSavedLayout } from "@/hooks/useSavedLayout";
import { useUiPrefs } from "@/state/uiPrefsStore";

const DEFAULT_PX = 248;
export const SIDEBAR_ID = "shell-sidebar";

/**
 * The sidebar's widths (tokens.css has the default and the icon rail).
 * Below 1280px app.css pins it to the icon rail and there is no handle.
 */
export const SIDEBAR_SIZE = {
  collapsedSize: "56px",
  minSize: "200px",
  defaultSize: `${DEFAULT_PX}px`,
  maxSize: "400px",
} as const;

/**
 * Keeps the resizable sidebar and the collapse preference in step. The
 * button (and the saved preference on load) collapse or expand the panel;
 * a drag past the minimum collapses it, and a drag out of the rail expands
 * it, and the preference follows. `narrow` is the pinned icon-rail width.
 */
export function useResizableSidebar(narrow: boolean) {
  const collapsedPref = useUiPrefs((s) => s.sidebarCollapsed);
  const panelRef = usePanelRef();
  const frameRef = useRef<HTMLDivElement>(null);
  // The size a drag or key press left it at, as a percentage of the frame.
  // Not every resize: a drag into the rail passes through the minimum.
  const lastSize = useRef<number | null>(null);
  // Read from the library's callbacks, which fire outside render.
  const narrowRef = useRef(narrow);
  useLayoutEffect(() => {
    narrowRef.current = narrow;
  }, [narrow]);
  const savedLayout = useSavedLayout("shell-sidebar", { save: !narrow });

  const onResize = useCallback(
    (size: PanelSize, _id: string | number | undefined, previous: PanelSize | undefined) => {
      const panel = panelRef.current;
      // While app.css pins the rail, the DOM width says nothing about the
      // panel's own size, and the panel can't be dragged.
      if (!panel || narrowRef.current) return;
      const prefs = useUiPrefs.getState();
      const collapsed = panel.isCollapsed();
      if (!collapsed && !prefs.sidebarCollapsed) {
        // Overlays placed beside the sidebar (the promise tray) read this.
        frameRef.current?.style.setProperty("--shell-sidebar-w", `${size.inPixels}px`);
      }
      // Skipped on mount, so the saved preference, not a saved layout,
      // decides how the sidebar opens.
      if (previous && collapsed !== prefs.sidebarCollapsed) prefs.setSidebarCollapsed(collapsed);
    },
    [panelRef],
  );

  useLayoutEffect(() => {
    const panel = panelRef.current;
    if (!panel) return;
    if (collapsedPref && !panel.isCollapsed()) panel.collapse();
    else if (!collapsedPref && panel.isCollapsed()) {
      // Back to the width it had, or the default after a reload.
      panel.resize(lastSize.current === null ? `${DEFAULT_PX}px` : `${lastSize.current}%`);
    }
  }, [collapsedPref, panelRef]);

  const { onLayoutChanged: save } = savedLayout;
  const onLayoutChanged = useCallback(
    (layout: Layout) => {
      save?.(layout);
      const size = layout[SIDEBAR_ID];
      if (size !== undefined && !narrowRef.current && !panelRef.current?.isCollapsed()) {
        lastSize.current = size;
      }
    },
    [panelRef, save],
  );

  return { panelRef, frameRef, onResize, groupProps: { ...savedLayout, onLayoutChanged } };
}
