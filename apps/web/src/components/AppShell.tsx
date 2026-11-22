import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import { lazy, Suspense, useEffect, useRef, useState, type ComponentType } from "react";

import { ArchiveRecoveryPanel } from "@/features/mail-actions/ArchiveRecoveryPanel";
import { AssistantSplit } from "@/features/assistant/AssistantSplit";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { MobileTabs } from "@/components/MobileTabs";
import { MoreSheet } from "@/components/MoreSheet";
import { OfflineBanner } from "@/components/OfflineBanner";
import { RecoveryNotice } from "@/features/recovery/RecoveryNotice";
import { Sidebar } from "@/components/Sidebar";
import { StatusBar } from "@/components/StatusBar";
import { ShortcutGuide } from "@/components/ShortcutGuide";
import { Topbar } from "@/components/Topbar";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { fetchAccounts } from "@/features/accounts/api";
import { llmStatusQuery } from "@/features/llm/useLlmStatus";
import { useNewMessageNotifier } from "@/features/notifications/useNewMessageNotifier";
import { chimeSettingsQuery } from "@/features/sound/api";
import { installSoundFeedback } from "@/features/sound/feedback";
import { useDesktopMenu } from "@/hooks/useDesktopMenu";
import { useKeyDispatcher } from "@/hooks/useKeyDispatcher";
import { NARROW_SHELL_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { SIDEBAR_ID, SIDEBAR_SIZE, useResizableSidebar } from "@/hooks/useResizableSidebar";
import { setRuntimeNavigate } from "@/lib/actions";
import { installKeyHints } from "@/lib/actions/keyHints";
import { useComposeUi } from "@/features/compose/composeUiStore";
import { useNewMessageEvent } from "@/features/compose/useNewMessageEvent";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { usePromiseOffers } from "@/features/promises/promiseOffers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { routeTitleForPath } from "@/lib/routePresentation";

const loadPalette = () => import("@/features/command-palette/CommandPalette");
const loadSearchPalette = () => import("@/features/search/SearchPalette");
const HelpDialog = lazyNamed(() => import("@/components/HelpDialog"), "HelpDialog");
const RightRail = lazyNamed(() => import("@/components/RightRail"), "RightRail");
const CommandPaletteMount = lazyNamed(loadPalette, "CommandPaletteMount");
const ComposeHost = lazyNamed(() => import("@/features/compose/ComposeHost"), "ComposeHost");
const ComposeLauncher = lazyNamed(
  () => import("@/features/compose/ComposeLauncher"),
  "ComposeLauncher",
);
const MailDialogs = lazyNamed(() => import("@/features/mail-actions/MailDialogs"), "MailDialogs");
const PromiseTray = lazyNamed(() => import("@/features/promises/PromiseTray"), "PromiseTray");
const SearchPalette = lazyNamed(loadSearchPalette, "SearchPalette");

function usePreloadPalettes(): void {
  useEffect(() => {
    const preload = () => {
      void loadPalette();
      void loadSearchPalette();
    };
    if ("requestIdleCallback" in window) {
      const handle = window.requestIdleCallback(preload, { timeout: 2000 });
      return () => window.cancelIdleCallback(handle);
    }
    const timer = setTimeout(preload, 500);
    return () => clearTimeout(timer);
  }, []);
}

function lazyNamed<M, K extends keyof M>(load: () => Promise<M>, name: K) {
  type Props = M[K] extends ComponentType<infer P> ? P : never;
  return lazy(async () => ({ default: (await load())[name] as ComponentType<Props> }));
}

function useOnceTrue(value: boolean): boolean {
  const [seen, setSeen] = useState(value);
  if (value && !seen) setSeen(true);
  return seen || value;
}

export function AppShell() {
  const queryClient = useQueryClient();
  useEffect(() => {
    void queryClient.prefetchQuery(llmStatusQuery);
    void queryClient.prefetchQuery(chimeSettingsQuery);
  }, [queryClient]);
  useEffect(() => installSoundFeedback(), []);
  useEffect(() => installKeyHints(), []);
  const sidebarCollapsed = useUiPrefs((s) => s.sidebarCollapsed);
  const narrow = useMediaQuery(NARROW_SHELL_QUERY);
  const rightRail = useModals((s) => s.rightRail);
  const helpOpen = useModals((s) => s.helpOpen);
  const setHelpOpen = useModals((s) => s.setHelpOpen);
  const navigate = useNavigate();
  const path = useRouterState({ select: (state) => state.location.pathname });
  const routeTitle = routeTitleForPath(path);
  const [routeAnnouncement, setRouteAnnouncement] = useState({ path: "", title: "" });
  useEffect(() => {
    document.title = `${routeTitle} · mxr`;
  }, [routeTitle]);
  useEffect(() => {
    const timer = window.setTimeout(() => setRouteAnnouncement({ path, title: routeTitle }), 50);
    return () => window.clearTimeout(timer);
  }, [path, routeTitle]);
  const announcedRouteTitle = routeAnnouncement.path === path ? routeAnnouncement.title : "";
  useNewMessageNotifier();
  usePreloadPalettes();
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    retry: false,
    staleTime: 60_000,
  });
  const noAccounts = accounts.data?.accounts.length === 0;
  const [keepOnboardingFrame, setKeepOnboardingFrame] = useState(false);
  if (path === "/onboarding" && noAccounts && !keepOnboardingFrame) {
    setKeepOnboardingFrame(true);
  } else if (path !== "/onboarding" && !noAccounts && keepOnboardingFrame) {
    setKeepOnboardingFrame(false);
  }

  useEffect(() => {
    setRuntimeNavigate({
      navigate: (to) => void navigate(to.includes("?") ? { href: to } : { to }),
    });
  }, [navigate]);
  useKeyDispatcher();

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
  const sidebar = useResizableSidebar(narrow);
  const paletteOpen = useModals((s) => s.commandPaletteOpen);
  const searchOpen = useModals((s) => s.searchPaletteOpen);
  const launcherOpen = useModals((s) => s.composeLauncherOpen);
  const composing = useComposeUi((s) => s.intent !== null);
  const mailDialog = useMailDialogs((s) => s.dialog !== null);
  const promiseOffered = usePromiseOffers((s) => s.offers.length > 0);
  const mountPalette = useOnceTrue(paletteOpen);
  const mountSearch = useOnceTrue(searchOpen);
  const mountLauncher = useOnceTrue(launcherOpen);
  useNewMessageEvent();
  useDesktopMenu();
  const mountCompose = useOnceTrue(composing);
  const mountDialogs = useOnceTrue(mailDialog);
  const mountHelp = useOnceTrue(helpOpen);
  const mountPromises = useOnceTrue(promiseOffered);
  const mountMore = useOnceTrue(useModals((s) => s.moreSheetOpen));

  const onboardingOnlyShell = noAccounts || (keepOnboardingFrame && path === "/onboarding");

  return (
    <>
      <ArchiveRecoveryPanel />
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {announcedRouteTitle}
      </div>
      {onboardingOnlyShell ? (
        <div className="flex h-dvh min-h-0 flex-col bg-background">
          <header className="flex h-12 shrink-0 items-center border-b border-border px-6">
            <span className="font-mono text-sm font-semibold">mxr</span>
          </header>
          <main id="main" className="flex min-h-0 flex-1 flex-col">
            <ErrorBoundary resetKey={path}>
              <Outlet />
            </ErrorBoundary>
          </main>
          <Suspense fallback={null}>
            {mountHelp ? <HelpDialog open={helpOpen} onOpenChange={setHelpOpen} /> : null}
          </Suspense>
        </div>
      ) : (
        <>
          <a
            href="#main"
            className="sr-only focus:not-sr-only focus:fixed focus:left-2 focus:top-2 focus:z-50 focus:rounded focus:bg-primary focus:px-3 focus:py-1.5 focus:text-primary-foreground"
          >
            Skip to content
          </a>
          <ResizablePanelGroup
            className="app-frame"
            style={{ height: "100dvh" }}
            data-sidebar-collapsed={sidebarCollapsed ? "true" : "false"}
            elementRef={sidebar.frameRef}
            {...sidebar.groupProps}
          >
            <ResizablePanel
              id={SIDEBAR_ID}
              data-shell-pane="sidebar"
              collapsible
              collapsedSize={SIDEBAR_SIZE.collapsedSize}
              minSize={SIDEBAR_SIZE.minSize}
              defaultSize={sidebar.defaultSize}
              maxSize={SIDEBAR_SIZE.maxSize}
              groupResizeBehavior="preserve-pixel-size"
              panelRef={sidebar.panelRef}
              onResize={sidebar.onResize}
              className="flex min-h-0 flex-col"
            >
              <aside className="app-shell-sidebar" aria-label="Mailboxes">
                <Sidebar collapsed={collapsed} />
              </aside>
            </ResizablePanel>
            {narrow ? null : (
              <ResizableHandle
                aria-label="Resize sidebar"
                className="bg-sidebar-border"
                {...sidebar.handleProps}
              />
            )}
            <ResizablePanel id="shell-body" className="flex min-h-0 min-w-0">
              <AssistantSplit>
                <div
                  className="app-shell"
                  data-sidebar-collapsed={sidebarCollapsed ? "true" : "false"}
                  data-rightrail-open={rightRail ? "true" : "false"}
                >
                  <header className="app-shell-topbar">
                    <Topbar />
                  </header>
                  {/* A press in a page takes the keys back from the sidebar
                      (`h` gave it them); the list and reader name themselves. */}
                  <main
                    id="main"
                    className="app-shell-main"
                    onMouseDown={() => {
                      const pane = useMailboxPane.getState();
                      if (pane.activePane === "sidebar") pane.setActivePane("mailbox");
                    }}
                  >
                    <RecoveryNotice />
                    <OfflineBanner />
                    <ErrorBoundary resetKey={path}>
                      <Outlet />
                    </ErrorBoundary>
                  </main>
                  {rightRail ? (
                    <aside className="app-shell-rightrail" aria-label="Context">
                      <Suspense fallback={null}>
                        <RightRail />
                      </Suspense>
                    </aside>
                  ) : null}
                  <footer className="app-shell-statusbar">
                    <StatusBar />
                  </footer>
                  <MobileTabs />
                  <ShortcutGuide />
                  {mountMore ? <MoreSheet /> : null}
                  <Suspense fallback={null}>
                    {mountPalette ? <CommandPaletteMount /> : null}
                    {mountLauncher ? <ComposeLauncher /> : null}
                    {mountCompose ? <ComposeHost /> : null}
                    {mountSearch ? <SearchPalette /> : null}
                    {mountDialogs ? <MailDialogs /> : null}
                    {mountHelp ? <HelpDialog open={helpOpen} onOpenChange={setHelpOpen} /> : null}
                    {mountPromises ? <PromiseTray /> : null}
                  </Suspense>
                </div>
              </AssistantSplit>
            </ResizablePanel>
          </ResizablePanelGroup>
        </>
      )}
    </>
  );
}
