import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import {
  Archive,
  BarChart3,
  Bookmark,
  CalendarDays,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Clock,
  FileText,
  History,
  Hourglass,
  Inbox,
  ListChecks,
  MailX,
  Package,
  Reply,
  Search,
  Send,
  Settings,
  Shield,
  ShieldAlert,
  Star,
  Stethoscope,
  Tag,
  Trash2,
  Users,
  Timer,
  Workflow,
} from "lucide-react";
import { Fragment, useEffect, useMemo, useRef, type ComponentType } from "react";

import { AccountSwitcher } from "@/components/AccountSwitcher";
import { railNavEntries } from "@/components/sidebarRail";
import { ThemePicker } from "@/components/ThemePicker";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useDeskQuery, type Desk } from "@/features/desk/api";
import { lensesFromShell, type MailLens } from "@/features/mailbox/lenses";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
import { useRailQuery } from "@/features/modes/rail";
import { fetchReplyQueue } from "@/features/reply-queue/api";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { plural } from "@/lib/format";
import { formatChord } from "@/lib/keys/chord";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Icon = ComponentType<{ className?: string }>;

export interface NavEntry {
  key: string;
  to: string;
  /** Query the place is defined by (Messages, waiting on them). */
  search?: { turn: "theirs" } | { account?: string };
  /** Says what a count covers when the page it opens shows less of it. */
  hint?: string;
  label: string;
  Icon: Icon;
  /** Only work carries a count: owed and due, the reply queue, screening. */
  count?: number;
  /** A quiet count (what a mode holds), not work: muted, never a dot. */
  quietCount?: boolean;
  /** "Early version: …": the mode is built on an existing view. */
  early?: string;
  shortcut?: string;
}

interface NavSection {
  id: string;
  title?: string;
  foldable: boolean;
  entries: NavEntry[];
}

const SYSTEM_ICONS: Record<string, Icon> = {
  inbox: Inbox,
  starred: Star,
  sent: Send,
  archive: Archive,
  spam: ShieldAlert,
  trash: Trash2,
};

const SYSTEM_SHORTCUTS: Record<string, string> = {
  inbox: "g i",
  starred: "g s",
  sent: "g t",
  archive: "g a",
  spam: "g !",
  trash: "g #",
};

/** Rarer triage lists under "More", after the ones the modes replaced. */
const MORE_TRIAGE: NavEntry[] = [
  { key: "owed", to: "/owed", label: "Owed replies", Icon: Hourglass, shortcut: "g o" },
  { key: "invites", to: "/invites", label: "Invites", Icon: CalendarDays, shortcut: "g v" },
  { key: "deliveries", to: "/deliveries", label: "Deliveries", Icon: Package, shortcut: "7" },
];

const TOOLS: NavEntry[] = [
  { key: "search", to: "/search", label: "Search", Icon: Search, shortcut: "2" },
  { key: "analytics", to: "/analytics", label: "Analytics", Icon: BarChart3, shortcut: "g A" },
  { key: "rules", to: "/rules", label: "Rules", Icon: Workflow, shortcut: "3" },
  { key: "accounts", to: "/accounts", label: "Accounts", Icon: Users, shortcut: "4" },
  { key: "activity", to: "/activity", label: "Activity", Icon: History, shortcut: "g y" },
  { key: "jobs", to: "/jobs", label: "Jobs", Icon: ListChecks },
  {
    key: "diagnostics",
    to: "/diagnostics",
    label: "Diagnostics",
    Icon: Stethoscope,
    shortcut: "5",
  },
  { key: "settings", to: "/settings/theme", label: "Settings", Icon: Settings, shortcut: "g c" },
];

/** Every system folder except the inbox, which is a place of its own. */
function folderEntries(lenses: MailLens[]): NavEntry[] {
  const entries: NavEntry[] = lenses
    .filter((lens) => lens.section === "system" && lens.key !== "drafts" && lens.key !== "inbox")
    .map((lens) => ({
      key: lens.key,
      to: lens.path,
      label: lens.label,
      Icon: SYSTEM_ICONS[lens.key] ?? Inbox,
      shortcut: SYSTEM_SHORTCUTS[lens.key],
    }));
  // Drafts has its own page, not a label lens.
  const afterSent = entries.findIndex((entry) => entry.key === "sent") + 1;
  entries.splice(afterSent > 0 ? afterSent : entries.length, 0, {
    key: "drafts",
    to: "/drafts",
    label: "Drafts",
    Icon: FileText,
    shortcut: "g d",
  });
  return entries;
}

/**
 * Screener, only while someone new waits for a decision. Same rule as the
 * desk's link: it opens the first account with senders waiting. Across
 * every account the count is a sum, so the entry says so.
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
      : `${plural(count, "new sender")} across your accounts; opens the first account with any`,
    shortcut: "g S",
  };
}

/** Sections whose entries open a mail list, so the keyboard follows. */
const LIST_SECTIONS = new Set(["places", "saved", "more", "labels"]);

function isActive(path: string, lane: unknown, entry: NavEntry): boolean {
  const base = entry.to;
  if (base === "/settings/theme") return path.startsWith("/settings");
  const onPath = path === base || path.startsWith(`${base}/`);
  // Messages and "Waiting on" share a path; the turn tells them apart.
  if (base === "/messages") {
    const entryTurn = entry.search && "turn" in entry.search ? entry.search.turn : null;
    return onPath && entryTurn === (lane === "theirs" ? "theirs" : null);
  }
  return onPath;
}

export function Sidebar({ collapsed }: { collapsed: boolean }) {
  const setCollapsed = useUiPrefs((s) => s.setSidebarCollapsed);
  const userCollapsed = useUiPrefs((s) => s.sidebarCollapsed);
  const folded = useUiPrefs((s) => s.collapsedSections);
  const toggleSection = useUiPrefs((s) => s.toggleSection);
  const setSectionCollapsed = useUiPrefs((s) => s.setSectionCollapsed);
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
  const shell = useShellQuery();
  const activePane = useMailboxPane((s) => s.activePane);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const focusIndex = useMailboxPane((s) => s.sidebarIndex);
  const setFocusIndex = useMailboxPane((s) => s.setSidebarIndex);
  const listRef = useRef<HTMLDivElement>(null);

  const desk = useDeskQuery();
  const rail = useRailQuery();
  const replyQueue = useQuery({ queryKey: ["reply-queue"], queryFn: fetchReplyQueue });

  const sections = useMemo<NavSection[]>(() => {
    const lenses = lensesFromShell(shell.data);
    const labels = lenses.filter((lens) => lens.section === "labels");
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
    // The views the modes replaced, still a key away under More. The
    // Screener's page is decision history; new senders are asked on their row.
    const screener: NavEntry = (desk.data ? screenerEntry(desk.data) : null) ?? {
      key: "screener",
      to: "/screener",
      label: "Screener",
      Icon: Shield,
      shortcut: "g S",
    };
    const replaced: NavEntry[] = [
      screener,
      {
        key: "reply-queue",
        to: "/reply-queue",
        label: "Reply queue",
        Icon: Reply,
        count: replyQueue.data?.messages.length,
        shortcut: "g q",
      },
      {
        key: "waiting",
        to: "/messages",
        search: { turn: "theirs" },
        label: "Waiting on",
        Icon: Timer,
        shortcut: "g w",
      },
      { key: "snoozed", to: "/snoozed", label: "Snoozed", Icon: Clock, shortcut: "g n" },
      { key: "subscriptions", to: "/subscriptions", label: "Subscriptions", Icon: MailX },
    ];
    const result: NavSection[] = [{ id: "places", foldable: false, entries: places }];
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
    result.push({
      id: "more",
      title: "More",
      foldable: true,
      entries: [...replaced, ...folderEntries(lenses), ...MORE_TRIAGE],
    });
    if (labels.length > 0) {
      result.push({
        id: "labels",
        title: "Labels",
        foldable: true,
        entries: labels.map((lens) => ({
          key: lens.key,
          to: lens.path,
          label: lens.label,
          Icon: Tag,
        })),
      });
    }
    result.push({ id: "tools", title: "Tools", foldable: true, entries: TOOLS });
    return result;
  }, [desk.data, rail.data, replyQueue.data, shell.data]);

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
    const index = visible.findIndex(({ entry }) => isActive(path, lane, entry));
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
      <div className="border-b border-sidebar-border p-2">
        <AccountSwitcher collapsed={collapsed} />
      </div>

      <div
        ref={listRef}
        className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-2 py-2"
        data-active-pane={sidebarFocused ? "true" : undefined}
      >
        {sections.map((section) => {
          // Folded groups stay folded in the icon rail too: it shows places.
          const isFolded = section.foldable && folded.includes(section.id);
          if (collapsed && isFolded) return null;
          return (
            <nav
              key={section.id}
              aria-label={section.title ?? "Places"}
              className={cn(section.id !== "places" && "mt-3")}
            >
              {section.title && !collapsed ? (
                <button
                  type="button"
                  onClick={() => toggleSection(section.id)}
                  aria-expanded={!isFolded}
                  className="group mb-0.5 flex w-full items-center gap-1 rounded px-2 py-1 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground hover:text-sidebar-foreground"
                >
                  <ChevronRight
                    className={cn("size-3 transition-transform", !isFolded && "rotate-90")}
                  />
                  {section.title}
                </button>
              ) : section.title && collapsed ? (
                <div className="mx-3 my-2 border-t border-sidebar-border" aria-hidden />
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
                        active={isActive(path, lane, entry)}
                        focused={sidebarFocused && clamp(focusIndex) === index}
                      />
                    );
                    // The rail's groups: Now, the modes, then Inbox (a lens).
                    return section.id === "places" &&
                      (entry.key === "messages" || entry.key === "inbox") ? (
                      <Fragment key={entry.key}>
                        <div aria-hidden className="mx-2 my-1.5 border-t border-sidebar-border" />
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
          "flex border-t border-sidebar-border p-1.5",
          collapsed ? "flex-col items-center gap-1" : "items-center justify-between",
        )}
      >
        <ThemePicker />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setCollapsed(!userCollapsed)}
              aria-label={userCollapsed ? "Expand sidebar" : "Collapse sidebar"}
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
      title={entry.hint}
      onClick={onActivate}
      data-nav-index={index}
      aria-current={active ? "page" : undefined}
      aria-label={collapsed ? `${entry.label}${count ? `, ${count}` : ""}` : undefined}
      className={cn(
        "group relative flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px]",
        collapsed && "justify-center px-0",
        active
          ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
          : "text-sidebar-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground",
        focused && "ring-1 ring-inset ring-sidebar-ring",
      )}
    >
      {active ? (
        <span
          aria-hidden
          className="absolute inset-y-1.5 left-0 w-0.5 rounded-full bg-sidebar-primary"
        />
      ) : null}
      <entry.Icon
        className={cn("size-4 shrink-0", active ? "text-sidebar-primary" : "text-muted-foreground")}
      />
      {!collapsed ? (
        <span className="min-w-0 flex-1 truncate">
          {entry.label}
          {entry.early ? (
            <span
              data-testid="rail-early"
              className="ml-1.5 font-mono text-[9.5px] uppercase tracking-wide text-muted-foreground"
            >
              early
            </span>
          ) : null}
        </span>
      ) : null}
      {!collapsed && entry.shortcut ? (
        <span className="hidden font-mono text-2xs text-muted-foreground group-hover:inline">
          {formatChord(entry.shortcut)}
        </span>
      ) : null}
      {!collapsed && count ? (
        <span
          data-testid="rail-count"
          className={cn(
            "font-mono text-2xs tabular-nums group-hover:hidden",
            entry.quietCount ? "text-muted-foreground" : "font-semibold text-sidebar-primary",
          )}
        >
          {count > 9999 ? "9999+" : count.toLocaleString()}
        </span>
      ) : null}
      {collapsed && count && !entry.quietCount ? (
        <span aria-hidden className="absolute right-1.5 top-1.5 size-1.5 rounded-full bg-primary" />
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
          <span className="ml-2 font-mono text-muted-foreground">
            {formatChord(entry.shortcut)}
          </span>
        ) : null}
      </TooltipContent>
    </Tooltip>
  );
}
