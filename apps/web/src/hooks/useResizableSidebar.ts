import { useCallback, useLayoutEffect, useRef, useState, type MouseEvent } from "react";
import { usePanelRef, type Layout, type PanelSize } from "react-resizable-panels";

import { panesWidth, readPaneWidth, writePaneWidth } from "@/hooks/paneWidth";
import { useUiPrefs } from "@/state/uiPrefsStore";

const DEFAULT_PX = 248;
export const SIDEBAR_ID = "shell-sidebar";
const STORAGE_KEY = "mxr:split:shell-sidebar";

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
 *
 * Its width is saved in pixels. While app.css pins the rail the library
 * still sizes the panel against the narrow window, and coming back from it
 * the library's guess can be far off, so the width is put back then.
 */
export function useResizableSidebar(narrow: boolean) {
  const collapsedPref = useUiPrefs((s) => s.sidebarCollapsed);
  const panelRef = usePanelRef();
  const frameRef = useRef<HTMLDivElement>(null);
  const [saved] = useState(() => readPaneWidth(STORAGE_KEY));
  // The width a drag or key press left it at. Not every resize: a drag into
  // the rail passes through the minimum, and the narrow window lies.
  const width = useRef(saved ?? DEFAULT_PX);
  // Read from the library's callbacks, which fire outside render.
  const narrowRef = useRef(narrow);
  const restoring = useRef(false);

  useLayoutEffect(() => {
    narrowRef.current = narrow;
    if (narrow) return;
    // The library hears of the wider window from its own observer: put the
    // width back a frame later, and save nothing until then.
    restoring.current = true;
    const frame = requestAnimationFrame(() => {
      const panel = panelRef.current;
      if (panel && !panel.isCollapsed()) panel.resize(`${width.current}px`);
      restoring.current = false;
    });
    return () => {
      cancelAnimationFrame(frame);
      restoring.current = false;
    };
  }, [narrow, panelRef]);

  const onResize = useCallback(
    (size: PanelSize, _id: string | number | undefined, previous: PanelSize | undefined) => {
      const panel = panelRef.current;
      // While app.css pins the rail, the DOM width says nothing about the
      // panel's own size, and the panel can't be dragged.
      if (!panel || narrowRef.current || restoring.current) return;
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
    else if (!collapsedPref && panel.isCollapsed()) panel.resize(`${width.current}px`);
  }, [collapsedPref, panelRef]);

  const onLayoutChanged = useCallback(
    (layout: Layout) => {
      const share = layout[SIDEBAR_ID];
      const panel = panelRef.current;
      if (share === undefined || !panel || narrowRef.current || restoring.current) return;
      if (panel.isCollapsed()) return;
      const next = Math.round((share / 100) * panesWidth(frameRef.current));
      if (next === width.current) return;
      width.current = next;
      writePaneWidth(STORAGE_KEY, next);
    },
    [panelRef],
  );

  return {
    panelRef,
    frameRef,
    onResize,
    groupProps: { onLayoutChanged },
    // The library resets to `defaultSize`, which here may be the saved
    // width; this resets to the real default.
    handleProps: {
      disableDoubleClick: true,
      onDoubleClick: (event: MouseEvent) => {
        event.preventDefault();
        panelRef.current?.resize(SIDEBAR_SIZE.defaultSize);
      },
    },
    // The saved width paints on the first frame.
    defaultSize: saved === null ? SIDEBAR_SIZE.defaultSize : `${saved}px`,
  };
}
