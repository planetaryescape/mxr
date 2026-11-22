import { useCallback, useLayoutEffect, useRef, useState, type MouseEvent } from "react";
import { usePanelRef, type Layout, type PanelSize } from "react-resizable-panels";

import { panesWidth, readPaneWidth, writePaneWidth } from "@/hooks/paneWidth";
import { useUiPrefs } from "@/state/uiPrefsStore";

const DEFAULT_PX = 248;
export const SIDEBAR_ID = "shell-sidebar";
const STORAGE_KEY = "mxr:split:shell-sidebar";

export const SIDEBAR_SIZE = {
  collapsedSize: "56px",
  minSize: "200px",
  defaultSize: `${DEFAULT_PX}px`,
  maxSize: "400px",
} as const;

export function useResizableSidebar(narrow: boolean) {
  const collapsedPref = useUiPrefs((s) => s.sidebarCollapsed);
  const panelRef = usePanelRef();
  const frameRef = useRef<HTMLDivElement>(null);
  const [saved] = useState(() => readPaneWidth(STORAGE_KEY));
  const width = useRef(saved ?? DEFAULT_PX);
  const narrowRef = useRef(narrow);
  const restoring = useRef(false);

  useLayoutEffect(() => {
    narrowRef.current = narrow;
    if (narrow) return;
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
      if (!panel || narrowRef.current || restoring.current) return;
      const prefs = useUiPrefs.getState();
      const collapsed = panel.isCollapsed();
      if (!collapsed && !prefs.sidebarCollapsed) {
        frameRef.current?.style.setProperty("--shell-sidebar-w", `${size.inPixels}px`);
      }
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
    handleProps: {
      disableDoubleClick: true,
      onDoubleClick: (event: MouseEvent) => {
        event.preventDefault();
        panelRef.current?.resize(SIDEBAR_SIZE.defaultSize);
      },
    },
    defaultSize: saved === null ? SIDEBAR_SIZE.defaultSize : `${saved}px`,
  };
}
