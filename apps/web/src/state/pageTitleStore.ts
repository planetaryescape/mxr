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
