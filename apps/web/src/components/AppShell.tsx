import { useQuery } from "@tanstack/react-query";
import { Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import { useEffect, useMemo } from "react";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { HelpDialog } from "@/components/HelpDialog";
import { OfflineBanner } from "@/components/OfflineBanner";
import { RightRail } from "@/components/RightRail";
import { Sidebar } from "@/components/Sidebar";
import { StatusBar } from "@/components/StatusBar";
import { Topbar } from "@/components/Topbar";
import { CommandPaletteMount } from "@/features/command-palette/CommandPalette";
import { ComposeLauncher } from "@/features/compose/ComposeLauncher";
import { SearchPalette } from "@/features/search/SearchPalette";
import { fetchAccounts } from "@/features/accounts/api";
import { useNewMessageNotifier } from "@/features/notifications/useNewMessageNotifier";
import { useKeybindings } from "@/hooks/useKeybindings";
import { buildGlobalKeymap } from "@/lib/keymap";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

export function AppShell() {
  const sidebarCollapsed = useUiPrefs((s) => s.sidebarCollapsed);
  const rightRail = useModals((s) => s.rightRail);
  const helpOpen = useModals((s) => s.helpOpen);
  const setHelpOpen = useModals((s) => s.setHelpOpen);
  const activePane = useMailboxPane((s) => s.activePane);
  const navigate = useNavigate();
  const path = useRouterState({ select: (state) => state.location.pathname });
  useNewMessageNotifier();
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    retry: false,
    staleTime: 60_000,
  });
  const keymap = useMemo(
    () => buildGlobalKeymap({ navigate: (to) => navigate({ to }) }),
    [navigate],
  );
  useKeybindings(keymap, { disabled: path.startsWith("/compose") });

  // Auto-close right rail on full route change to avoid stale context
  useEffect(() => {
    return () => useModals.getState().closeRightRail();
  }, []);

  useEffect(() => {
    if (path !== "/onboarding" && accounts.data?.accounts.length === 0) {
      void navigate({ to: "/onboarding" });
    }
  }, [accounts.data?.accounts.length, navigate, path]);

  return (
    <div
      className="app-shell"
      data-sidebar-collapsed={sidebarCollapsed ? "true" : "false"}
      data-rightrail-open={rightRail ? "true" : "false"}
    >
      <div className="app-shell-sidebar">
        <Sidebar />
      </div>
      <div className="app-shell-topbar">
        <Topbar />
      </div>
      <div className="app-shell-main">
        <OfflineBanner />
        <ErrorBoundary>
          <Outlet />
        </ErrorBoundary>
      </div>
      {rightRail ? (
        <div className="app-shell-rightrail">
          <RightRail />
        </div>
      ) : null}
      <div className="app-shell-statusbar">
        <StatusBar />
      </div>
      <CommandPaletteMount />
      <ComposeLauncher />
      <SearchPalette />
      <HelpDialog open={helpOpen} onOpenChange={setHelpOpen} path={path} activePane={activePane} />
    </div>
  );
}
