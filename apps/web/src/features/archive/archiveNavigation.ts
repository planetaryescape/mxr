import { defaultStringifySearch, useRouterState } from "@tanstack/react-router";
import { useEffect, useRef } from "react";
import { create } from "zustand";

import { useUiPrefs } from "@/state/uiPrefsStore";

import { parseArchiveSearch, type ArchiveSearch } from "./archiveSearch";

interface ArchiveNavigation {
  searches: Record<string, ArchiveSearch>;
  remember: (account: string | null, search: ArchiveSearch) => void;
}

export const useArchiveNavigation = create<ArchiveNavigation>((set) => ({
  searches: {},
  remember: (account, search) =>
    set((state) => {
      const key = account ?? "all";
      if (JSON.stringify(state.searches[key]) === JSON.stringify(search)) return state;
      return { searches: { ...state.searches, [key]: search } };
    }),
}));

export function useArchiveResumeSearch(): ArchiveSearch | undefined {
  const account = useUiPrefs((state) => state.accountScope);
  const location = useRouterState({ select: (state) => state.location });
  const search = useArchiveNavigation((state) => state.searches[account ?? "all"]);
  const remember = useArchiveNavigation((state) => state.remember);
  const recorded = useRef({ account, search: JSON.stringify(location.search) });
  useEffect(() => {
    if (location.pathname === "/archive" || location.pathname.startsWith("/archive/")) {
      const currentSearch = JSON.stringify(location.search);
      if (account !== recorded.current.account && currentSearch === recorded.current.search) return;
      recorded.current = { account, search: currentSearch };
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
