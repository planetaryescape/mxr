/*
 * The title of the page on screen, as its h1 publishes it. The Topbar reads
 * it so a lone breadcrumb that repeats the h1 is not shown twice.
 */

import { useLayoutEffect } from "react";
import { create } from "zustand";

interface PageTitleState {
  title: string | null;
  setTitle: (title: string | null) => void;
}

export const usePageTitleStore = create<PageTitleState>((set) => ({
  title: null,
  setTitle: (title) => set({ title }),
}));

/**
 * Publish while the h1 is mounted. Layout effect so the Topbar updates in the
 * same paint as the page. Cleanup clears only its own title: when one page
 * replaces another, the outgoing page must not wipe the incoming one.
 */
export function usePublishPageTitle(title: string | null) {
  useLayoutEffect(() => {
    if (title === null) return;
    const { setTitle } = usePageTitleStore.getState();
    setTitle(title);
    return () => {
      if (usePageTitleStore.getState().title === title) setTitle(null);
    };
  }, [title]);
}
