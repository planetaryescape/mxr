import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import {
  Bookmark,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Ellipsis,
  FolderOpen,
  Shield,
} from "lucide-react";
import { Fragment, useEffect, useMemo, useRef } from "react";

import { AccountSwitcher } from "@/components/AccountSwitcher";
import {
  isEntryActive,
  MAIL_ENTRIES,
  TOOL_ENTRIES,
  VIEW_ENTRIES,
  type NavEntry,
  type NavSection,
} from "@/components/navMap";
import { railNavEntries } from "@/components/sidebarRail";
import { StatusDot } from "@/components/StatusDot";
import { ThemePicker } from "@/components/ThemePicker";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useDeskQuery, type Desk } from "@/features/desk/api";
import { lensesFromShell } from "@/features/mailbox/lenses";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
import { useRailQuery } from "@/features/modes/rail";
import { labelColor } from "@/features/thread/LabelBadge";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { plural } from "@/lib/format";
import { formatChord } from "@/lib/keys/chord";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useModals } from "@/state/modalStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

/**
 * Screener, only while someone new waits for a decision. Same rule as the
 * desk's link: it opens the first account with senders waiting, and the
 * count is that account's, so the badge equals the page it opens.
 */
export function screenerEntry(desk: Desk): NavEntry | null {
  const count = desk.elsewhere.screener;
  if (count <= 0) return null;
  return {
    key: "screener",
    to: "/screener",
    search: { account: desk.elsewhere.screener_account ?? undefined },
    label: "Screener",
    Icon: Shield,
    count,
    hint: desk.account_id
      ? undefined
      : `${plural(count, "new sender")} in the first account with any; pick an account to see its own`,
    shortcut: "g S",
  };
}

/** Sections whose entries open a mail list, so the keyboard follows. */
const LIST_SECTIONS = new Set(["places", "saved", "mail", "views", "labels", "categories"]);

/**
 * The map's sections, in order: the rail (Now, the five modes, Inbox), then
 * Mail, Views, Tools and Labels (D133). Mail, Views and Tools are fixed data
 * and never depend on the daemon answering; only the Screener's count, the
 * mode counts, labels and saved searches do. The rail and the phone's More
 * sheet both read this.
 */
export function useNavSections(): NavSection[] {
  const shell = useShellQuery();
  const desk = useDeskQuery();
  const rail = useRailQuery();
  return useMemo<NavSection[]>(() => {
    const lenses = lensesFromShell(shell.data);
    const labels = lenses.filter((lens) => lens.section === "labels");
    const categories = lenses.filter((lens) => lens.section === "categories");
    const saved = lenses.filter((lens) => lens.section === "saved");
    // The rail, in the daemon's order: Now, the five modes, then Inbox.
    const places: NavEntry[] = railNavEntries(rail.data).map((entry) => ({
      key: entry.key,
      to: entry.to,
      label: entry.label,
      Icon: entry.Icon,
      shortcut: entry.shortcut,
      count: entry.count,
      quietCount: !entry.badge,
      early: entry.early,
      hint: [entry.header, entry.early].filter(Boolean).join(" ") || undefined,
    }));
    // The Screener's page is decision history; new senders are asked on their row.
    const screener = desk.data ? screenerEntry(desk.data) : null;
    const views = VIEW_ENTRIES.map((entry) =>
      entry.key === "screener" && screener ? screener : entry,
    );
    const result: NavSection[] = [
      { id: "places", foldable: false, entries: places },
      { id: "mail", title: "Mail", foldable: true, entries: MAIL_ENTRIES },
      { id: "views", title: "Views", foldable: true, entries: views },
      { id: "tools", title: "Tools", foldable: true, entries: TOOL_ENTRIES },
    ];
    if (labels.length > 0) {
      result.push({
        id: "labels",
        title: "Labels",
        foldable: true,
        entries: labels.map((lens) => ({
          key: lens.key,
          to: lens.path,
          label: lens.label,
          Icon: FolderOpen,
          color: labelColor(lens.label),
        })),
      });
    }
    if (saved.length > 0) {
      result.push({
        id: "saved",
        title: "Saved searches",
        foldable: true,
        entries: saved.map((lens, index) => ({
          key: lens.key,
          to: lens.path,
          label: lens.label,
          Icon: Bookmark,
          shortcut: index < 9 ? `g ${index + 1}` : undefined,
        })),
      });
    }
    if (categories.length > 0) {
      result.push({
        id: "categories",
        title: "Categories",
        foldable: true,
        entries: categories.map((lens) => ({
          key: lens.key,
          to: lens.path,
          label: lens.label,
          Icon: FolderOpen,
        })),
      });
    }
    return result;
  }, [desk.data, rail.data, shell.data]);
}

export function Sidebar({ collapsed }: { collapsed: boolean }) {
  const setCollapsed = useUiPrefs((s) => s.setSidebarCollapsed);
  const userCollapsed = useUiPrefs((s) => s.sidebarCollapsed);
  const folded = useUiPrefs((s) => s.collapsedSections);
  const toggleSection = useUiPrefs((s) => s.toggleSection);
  const setSectionCollapsed = useUiPrefs((s) => s.setSectionCollapsed);
  const setMoreOpen = useModals((s) => s.setMoreSheetOpen);
  const navigate = useNavigate();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const lane = useRouterState({
    select: (s) =>
      "turn" in s.location.search
        ? s.location.search.turn
        : "lane" in s.location.search
          ? s.location.search.lane
          : undefined,
  });
  const activePane = useMailboxPane((s) => s.activePane);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const focusIndex = useMailboxPane((s) => s.sidebarIndex);
  const setFocusIndex = useMailboxPane((s) => s.setSidebarIndex);
  const listRef = useRef<HTMLDivElement>(null);

  // The collapsed rail is the modes and Inbox only (D133): the rest is one
  // click away in the More sheet, so the narrow rail stays a rail.
  const allSections = useNavSections();
  const sections = useMemo(
    () => (collapsed ? allSections.filter((section) => section.id === "places") : allSections),
    [allSections, collapsed],
  );

  // Keyboard walks only what is visible: folded sections contribute their
  // header, not their entries.
  const visible = useMemo(
    () =>
      sections.flatMap((section) =>
        section.foldable && folded.includes(section.id)
          ? []
          : section.entries.map((entry) => ({ entry, section })),
      ),
    [folded, sections],
  );

  const sidebarFocused = activePane === "sidebar";
  useShortcutScope("sidebar", sidebarFocused);
  const clamp = (index: number) => Math.max(0, Math.min(visible.length - 1, index));
  const visibleCount = visible.length;
  const current = visible[clamp(focusIndex)];
  useScopeController("sidebar", {
    down: () => setFocusIndex(clamp(focusIndex + 1)),
    up: () => setFocusIndex(clamp(focusIndex - 1)),
    top: () => setFocusIndex(0),
    bottom: () => setFocusIndex(visible.length - 1),
    open: () => {
      if (!current) return;
      void navigate({ to: current.entry.to, search: current.entry.search });
      if (LIST_SECTIONS.has(current.section.id)) setActivePane("mailbox");
    },
    collapse: () =>
      current && current.section.foldable && setSectionCollapsed(current.section.id, true),
    expand: () => {
      const next = sections.find((section) => section.foldable && folded.includes(section.id));
      if (next) setSectionCollapsed(next.id, false);
    },
  });

  // Keep the keyboard cursor on the page the user is on when they arrive.
  useEffect(() => {
    if (sidebarFocused) return;
    const index = visible.findIndex(({ entry }) => isEntryActive(path, lane, entry));
    if (index >= 0 && index !== focusIndex) setFocusIndex(index);
  }, [focusIndex, lane, path, setFocusIndex, sidebarFocused, visible]);

  // Follow the keyboard cursor, only when it moves: scrolling on every
  // render would pull the list away from under a pointer mid-click.
  useEffect(() => {
    if (!sidebarFocused) return;
    const index = Math.max(0, Math.min(visibleCount - 1, focusIndex));
    listRef.current
      ?.querySelector<HTMLElement>(`[data-nav-index="${index}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [focusIndex, sidebarFocused, visibleCount]);

  let runningIndex = -1;
  return (
    <div
      className="flex h-full min-h-0 flex-col"
      onMouseDown={(event) => {
        // The pressed item becomes the cursor before the pane takes the
        // keyboard, so the cursor-follow scroll has nowhere else to go.
        const item = (event.target as Element).closest<HTMLElement>("[data-nav-index]");
        if (item) setFocusIndex(Number(item.dataset.navIndex));
        setActivePane("sidebar");
      }}
    >
      <div className="border-b border-rule-soft p-2">
        <AccountSwitcher collapsed={collapsed} />
      </div>

      <div
        ref={listRef}
        className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-2 py-2"
        data-active-pane={sidebarFocused ? "true" : undefined}
      >
        {sections.map((section) => {
          const isFolded = section.foldable && folded.includes(section.id);
          return (
            <nav
              key={section.id}
              aria-label={section.title ?? "Places"}
              className={cn(section.id !== "places" && "mt-3")}
            >
              {section.title ? (
                <button
                  type="button"
                  onClick={() => toggleSection(section.id)}
                  aria-expanded={!isFolded}
                  className="type-eyebrow group mb-0.5 flex w-full items-center gap-1 rounded-sm px-2 py-1 text-text-3 hover:text-text-2"
                >
                  <ChevronRight
                    className={cn("size-3 transition-transform", !isFolded && "rotate-90")}
                  />
                  {section.title}
                </button>
              ) : null}
              {isFolded
                ? null
                : section.entries.map((entry) => {
                    runningIndex += 1;
                    const index = runningIndex;
                    const link = (
                      <SidebarLink
                        key={entry.key}
                        entry={entry}
                        index={index}
                        onActivate={() => {
                          setFocusIndex(index);
                          // A clicked mailbox is where the keyboard goes next.
                          if (LIST_SECTIONS.has(section.id)) setActivePane("mailbox");
                        }}
                        collapsed={collapsed}
                        active={isEntryActive(path, lane, entry)}
                        focused={sidebarFocused && clamp(focusIndex) === index}
                      />
                    );
                    // The rail's groups: Now, the modes, then Inbox (a lens).
                    return section.id === "places" &&
                      (entry.key === "messages" || entry.key === "inbox") ? (
                      <Fragment key={entry.key}>
                        <div aria-hidden className="mx-2 my-1.5 border-t border-rule-soft" />
                        {link}
                      </Fragment>
                    ) : (
                      link
                    );
                  })}
            </nav>
          );
        })}
      </div>

      <div
        className={cn(
          "flex border-t border-rule-soft p-1.5",
          collapsed ? "flex-col items-center gap-1" : "items-center justify-between",
        )}
      >
        <ThemePicker />
        {collapsed ? (
          // The groups the narrow rail leaves out stay reachable on every width.
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => setMoreOpen(true)}
                aria-label="More places"
                data-more-trigger
                className="text-text-3"
              >
                <Ellipsis className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="right">Mail, views, tools and labels</TooltipContent>
          </Tooltip>
        ) : null}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setCollapsed(!userCollapsed)}
              aria-label={userCollapsed ? "Expand sidebar" : "Collapse sidebar"}
              className="text-text-3"
            >
              {userCollapsed ? (
                <ChevronsRight className="size-4" />
              ) : (
                <ChevronsLeft className="size-4" />
              )}
            </Button>
          </TooltipTrigger>
          <TooltipContent side="right">
            {userCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          </TooltipContent>
        </Tooltip>
      </div>
    </div>
  );
}

/** The mark before a nav label: its icon, or a label's own colour as a square. */
export function NavMark({ entry, className }: { entry: NavEntry; className?: string }) {
  if (entry.color) {
    return (
      <span
        aria-hidden
        className={cn("flex size-4 shrink-0 items-center justify-center", className)}
      >
        {/* A validated hex (labelColor), never raw data in a style value. */}
        <span className="size-1.5" style={{ backgroundColor: entry.color }} />
      </span>
    );
  }
  return <entry.Icon className={cn("size-4 shrink-0", className)} />;
}

function SidebarLink({
  entry,
  index,
  collapsed,
  active,
  focused,
  onActivate,
}: {
  entry: NavEntry;
  index: number;
  onActivate: () => void;
  collapsed: boolean;
  active: boolean;
  focused: boolean;
}) {
  const count = entry.count && entry.count > 0 ? entry.count : null;
  const link = (
    <Link
      to={entry.to}
      search={entry.search}
      // The router would call Messages current on Waiting on; the nav decides.
      activeOptions={{ exact: true }}
      title={entry.hint}
      onClick={onActivate}
      data-nav-index={index}
      aria-current={active ? "page" : undefined}
      aria-label={collapsed ? `${entry.label}${count ? `, ${count}` : ""}` : undefined}
      className={cn(
        "group relative flex h-8 items-center gap-2.5 px-2 text-sm",
        collapsed && "justify-center px-0",
        // The rail is dimmer than the page (D133): labels in the second text
        // tier, icons in the third; only the page you are on reaches the first.
        active
          ? "bg-ink-lamp font-medium text-text-1"
          : "text-text-2 hover:bg-ink-lamp/60 hover:text-text-1",
        focused && "ring-1 ring-inset ring-sidebar-ring",
      )}
    >
      <NavMark entry={entry} className={active ? "text-text-2" : "text-text-3"} />
      {!collapsed ? (
        <span className="min-w-0 flex-1 truncate">
          {entry.label}
          {entry.early ? (
            <span data-testid="rail-early" className="type-meta ml-1.5 uppercase text-text-3">
              early
            </span>
          ) : null}
        </span>
      ) : null}
      {!collapsed && entry.shortcut ? (
        <span className="type-meta hidden text-text-3 group-hover:inline">
          {formatChord(entry.shortcut)}
        </span>
      ) : null}
      {!collapsed && count ? (
        <span
          data-testid="rail-count"
          className={cn(
            "type-meta group-hover:hidden",
            entry.quietCount ? "text-text-3" : "font-semibold text-unread-marker",
          )}
        >
          {count > 9999 ? "9999+" : count.toLocaleString()}
        </span>
      ) : null}
      {collapsed && count && !entry.quietCount ? (
        <StatusDot className="absolute right-1.5 top-1.5 size-1.5 bg-unread-marker" />
      ) : null}
    </Link>
  );
  if (!collapsed) return link;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{link}</TooltipTrigger>
      <TooltipContent side="right">
        {entry.label}
        {entry.shortcut ? (
          <span className="ml-2 font-mono text-text-3">{formatChord(entry.shortcut)}</span>
        ) : null}
      </TooltipContent>
    </Tooltip>
  );
}
