import { defaultStringifySearch, useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";
import { create } from "zustand";

import { useUiPrefs } from "@/state/uiPrefsStore";

import { parseArchiveSearch, type ArchiveSearch } from "./archiveSearch";

interface ArchiveNavigation {
  searches: Record<string, ArchiveSearch>;
  remember: (account: string | null, search: ArchiveSearch) => void;
}

/** Tab-local navigation memory, not a persisted history of questions. */
export const useArchiveNavigation = create<ArchiveNavigation>((set) => ({
  searches: {},
  remember: (account, search) =>
    set((state) => {
      const key = account ?? "all";
      if (JSON.stringify(state.searches[key]) === JSON.stringify(search)) return state;
      return { searches: { ...state.searches, [key]: search } };
    }),
}));

/** Shared by the sidebar and phone's More sheet; explicit URLs still win. */
export function useArchiveResumeSearch(): ArchiveSearch | undefined {
  const account = useUiPrefs((state) => state.accountScope);
  const location = useRouterState({ select: (state) => state.location });
  const search = useArchiveNavigation((state) => state.searches[account ?? "all"]);
  const remember = useArchiveNavigation((state) => state.remember);
  useEffect(() => {
    if (location.pathname === "/archive" || location.pathname.startsWith("/archive/")) {
      remember(account, parseArchiveSearch(location.search));
    }
  }, [account, location.pathname, location.search, remember]);
  return search;
}

export function archiveResumeHref(): string {
  const account = useUiPrefs.getState().accountScope;
  const search = useArchiveNavigation.getState().searches[account ?? "all"];
  return `/archive${defaultStringifySearch(search ?? {})}`;
}
