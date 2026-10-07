import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import { lazy, Suspense, useEffect, useRef, useState, type ComponentType } from "react";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { MobileTabs } from "@/components/MobileTabs";
import { OfflineBanner } from "@/components/OfflineBanner";
import { Sidebar } from "@/components/Sidebar";
import { StatusBar } from "@/components/StatusBar";
import { Topbar } from "@/components/Topbar";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { fetchAccounts } from "@/features/accounts/api";
import { llmStatusQuery } from "@/features/llm/useLlmStatus";
import { useNewMessageNotifier } from "@/features/notifications/useNewMessageNotifier";
import { chimeSettingsQuery } from "@/features/sound/api";
import { installSoundFeedback } from "@/features/sound/feedback";
import { useKeyDispatcher } from "@/hooks/useKeyDispatcher";
import { NARROW_SHELL_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { SIDEBAR_ID, SIDEBAR_SIZE, useResizableSidebar } from "@/hooks/useResizableSidebar";
import { setRuntimeNavigate } from "@/lib/actions";
import { installKeyHints } from "@/lib/actions/keyHints";
import { useComposeUi } from "@/features/compose/composeUiStore";
import { useMailDialogs } from "@/features/mail-actions/mailDialogStore";
import { usePromiseOffers } from "@/features/promises/promiseOffers";
import { useModals } from "@/state/modalStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

// Everything below only exists while open, so it loads on first use and
// stays out of the entry chunk (compose alone pulls in editors, the HTML
// sanitizer and address parsing).
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

/**
 * The palettes open from a key and the user types straight on, so their
 * chunks must be ready before the first ⌘K or /: letters typed while a
 * chunk is still loading have nowhere to go.
 */
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

/** Mount once first needed, then keep mounted so state and focus survive. */
function useOnceTrue(value: boolean): boolean {
  const [seen, setSeen] = useState(value);
  if (value && !seen) setSeen(true);
  return seen || value;
}

export function AppShell() {
  // Fetch the model status once, early, so an opened thread (from any route)
  // already knows whether to reserve its gist slot. A prefetch, not a
  // subscription: its answer must never re-render the shell (and the list
  // under it). Nothing waits on it.
  const queryClient = useQueryClient();
  useEffect(() => {
    void queryClient.prefetchQuery(llmStatusQuery);
    // The sound setting, read by the player from the cache when it plays.
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
  useNewMessageNotifier();
  usePreloadPalettes();
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    retry: false,
    staleTime: 60_000,
  });

  useEffect(() => {
    // A target with a query string ("/focus?from=...") goes by href, which
    // the router parses; `to` is a path only.
    setRuntimeNavigate({
      navigate: (to) => void navigate(to.includes("?") ? { href: to } : { to }),
    });
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
  const mountCompose = useOnceTrue(composing);
  const mountDialogs = useOnceTrue(mailDialog);
  const mountHelp = useOnceTrue(helpOpen);
  const mountPromises = useOnceTrue(promiseOffered);

  return (
    <>
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:left-2 focus:top-2 focus:z-50 focus:rounded focus:bg-primary focus:px-3 focus:py-1.5 focus:text-primary-foreground"
      >
        Skip to content
      </a>
      <ResizablePanelGroup
        className="app-frame"
        // Inline: the group's own inline height (100%) beats a class.
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
          <div
            className="app-shell"
            data-sidebar-collapsed={sidebarCollapsed ? "true" : "false"}
            data-rightrail-open={rightRail ? "true" : "false"}
          >
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
                <Suspense fallback={null}>
                  <RightRail />
                </Suspense>
              </aside>
            ) : null}
            <footer className="app-shell-statusbar">
              <StatusBar />
            </footer>
            <MobileTabs />
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
        </ResizablePanel>
      </ResizablePanelGroup>
    </>
  );
}
