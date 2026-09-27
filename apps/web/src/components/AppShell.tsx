import { useQuery } from "@tanstack/react-query";
import { Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import { useEffect, useRef } from "react";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { HelpDialog } from "@/components/HelpDialog";
import { OfflineBanner } from "@/components/OfflineBanner";
import { RightRail } from "@/components/RightRail";
import { Sidebar } from "@/components/Sidebar";
import { StatusBar } from "@/components/StatusBar";
import { Topbar } from "@/components/Topbar";
import { fetchAccounts } from "@/features/accounts/api";
import { CommandPaletteMount } from "@/features/command-palette/CommandPalette";
import { ComposeHost } from "@/features/compose/ComposeHost";
import { ComposeLauncher } from "@/features/compose/ComposeLauncher";
import { MailDialogs } from "@/features/mail-actions/MailDialogs";
import { useNewMessageNotifier } from "@/features/notifications/useNewMessageNotifier";
import { SearchPalette } from "@/features/search/SearchPalette";
import { useKeyDispatcher } from "@/hooks/useKeyDispatcher";
import { NARROW_SHELL_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { setRuntimeNavigate } from "@/lib/actions";
import { useModals } from "@/state/modalStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

export function AppShell() {
  const sidebarCollapsed = useUiPrefs((s) => s.sidebarCollapsed);
  const narrow = useMediaQuery(NARROW_SHELL_QUERY);
  const rightRail = useModals((s) => s.rightRail);
  const helpOpen = useModals((s) => s.helpOpen);
  const setHelpOpen = useModals((s) => s.setHelpOpen);
  const navigate = useNavigate();
  const path = useRouterState({ select: (state) => state.location.pathname });
  useNewMessageNotifier();
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    retry: false,
    staleTime: 60_000,
  });

  useEffect(() => {
    setRuntimeNavigate({ navigate: (to) => void navigate({ to }) });
  }, [navigate]);
  useKeyDispatcher();

  // A rail shows context for what was on screen; leaving that page (not
  // just opening another thread in the same lens) closes it.
  const railSection = path.split("/").slice(0, 3).join("/");
  const lastSection = useRef(railSection);
  useEffect(() => {
    if (lastSection.current !== railSection) useModals.getState().closeRightRail();
    lastSection.current = railSection;
  }, [railSection]);

  useEffect(() => {
    if (path !== "/onboarding" && accounts.data?.accounts.length === 0) {
      void navigate({ to: "/onboarding" });
    }
  }, [accounts.data?.accounts.length, navigate, path]);

  const collapsed = sidebarCollapsed || narrow;

  return (
    <div
      className="app-shell"
      data-sidebar-collapsed={sidebarCollapsed ? "true" : "false"}
      data-rightrail-open={rightRail ? "true" : "false"}
    >
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:left-2 focus:top-2 focus:z-50 focus:rounded focus:bg-primary focus:px-3 focus:py-1.5 focus:text-primary-foreground"
      >
        Skip to content
      </a>
      <aside className="app-shell-sidebar" aria-label="Mailboxes">
        <Sidebar collapsed={collapsed} />
      </aside>
      <header className="app-shell-topbar">
        <Topbar />
      </header>
      <main id="main" className="app-shell-main">
        <OfflineBanner />
        <ErrorBoundary resetKey={path}>
          <Outlet />
        </ErrorBoundary>
      </main>
      {rightRail ? (
        <aside className="app-shell-rightrail" aria-label="Context">
          <RightRail />
        </aside>
      ) : null}
      <footer className="app-shell-statusbar">
        <StatusBar />
      </footer>
      <CommandPaletteMount />
      <ComposeLauncher />
      <ComposeHost />
      <SearchPalette />
      <MailDialogs />
      <HelpDialog open={helpOpen} onOpenChange={setHelpOpen} />
    </div>
  );
}
