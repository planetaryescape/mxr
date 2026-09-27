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
  Workflow,
} from "lucide-react";
import { useEffect, useMemo, useRef, type ComponentType } from "react";

import { AccountSwitcher } from "@/components/AccountSwitcher";
import { ThemePicker } from "@/components/ThemePicker";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { lensesFromShell, type MailLens } from "@/features/mailbox/lenses";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { formatChord } from "@/lib/keys/chord";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

type Icon = ComponentType<{ className?: string }>;

interface NavEntry {
  key: string;
  to: string;
  label: string;
  Icon: Icon;
  count?: number;
  /** Bold count: unread that deserves attention (Inbox, labels). */
  emphasize?: boolean;
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

const TRIAGE: NavEntry[] = [
  { key: "reply-queue", to: "/reply-queue", label: "Reply queue", Icon: Reply, shortcut: "g q" },
  { key: "owed", to: "/owed", label: "Owed replies", Icon: Hourglass, shortcut: "g o" },
  { key: "screener", to: "/screener", label: "Screener", Icon: Shield, shortcut: "g S" },
  { key: "invites", to: "/invites", label: "Invites", Icon: CalendarDays, shortcut: "g v" },
  {
    key: "subscriptions",
    to: "/subscriptions",
    label: "Subscriptions",
    Icon: MailX,
    shortcut: "g u",
  },
];

const TOOLS: NavEntry[] = [
  { key: "search", to: "/search", label: "Search", Icon: Search, shortcut: "2" },
  { key: "analytics", to: "/analytics", label: "Analytics", Icon: BarChart3, shortcut: "g A" },
  { key: "rules", to: "/rules", label: "Rules", Icon: Workflow, shortcut: "3" },
  { key: "deliveries", to: "/deliveries", label: "Deliveries", Icon: Package, shortcut: "7" },
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

function mailEntries(lenses: MailLens[]): NavEntry[] {
  const system = lenses.filter((lens) => lens.section === "system" && lens.key !== "drafts");
  const entries: NavEntry[] = system.map((lens) => ({
    key: lens.key,
    to: lens.path,
    label: lens.label,
    Icon: SYSTEM_ICONS[lens.key] ?? Inbox,
    count: lens.key === "inbox" ? lens.unread : lens.key === "starred" ? lens.total : undefined,
    emphasize: lens.key === "inbox",
    shortcut: SYSTEM_SHORTCUTS[lens.key],
  }));
  if (entries.length === 0) {
    entries.push({ key: "inbox", to: "/m/inbox", label: "Inbox", Icon: Inbox, shortcut: "g i" });
  }
  // Snooze and drafts have their own pages, not label lenses.
  const afterStarred = Math.max(1, entries.findIndex((entry) => entry.key === "starred") + 1);
  entries.splice(afterStarred, 0, {
    key: "snoozed",
    to: "/snoozed",
    label: "Snoozed",
    Icon: Clock,
    shortcut: "g n",
  });
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

function isActive(path: string, to: string): boolean {
  const base = to.split("?")[0] ?? to;
  if (base === "/settings/theme") return path.startsWith("/settings");
  return path === base || path.startsWith(`${base}/`);
}

export function Sidebar({ collapsed }: { collapsed: boolean }) {
  const setCollapsed = useUiPrefs((s) => s.setSidebarCollapsed);
  const userCollapsed = useUiPrefs((s) => s.sidebarCollapsed);
  const folded = useUiPrefs((s) => s.collapsedSections);
  const toggleSection = useUiPrefs((s) => s.toggleSection);
  const setSectionCollapsed = useUiPrefs((s) => s.setSectionCollapsed);
  const navigate = useNavigate();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const shell = useShellQuery();
  const activePane = useMailboxPane((s) => s.activePane);
  const setActivePane = useMailboxPane((s) => s.setActivePane);
  const focusIndex = useMailboxPane((s) => s.sidebarIndex);
  const setFocusIndex = useMailboxPane((s) => s.setSidebarIndex);
  const listRef = useRef<HTMLDivElement>(null);

  const sections = useMemo<NavSection[]>(() => {
    const lenses = lensesFromShell(shell.data);
    const labels = lenses.filter((lens) => lens.section === "labels");
    const saved = lenses.filter((lens) => lens.section === "saved");
    const result: NavSection[] = [
      { id: "mail", foldable: false, entries: mailEntries(lenses) },
      { id: "triage", title: "Triage", foldable: true, entries: TRIAGE },
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
          Icon: Tag,
          count: lens.unread,
          emphasize: true,
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
    result.push({ id: "tools", title: "Tools", foldable: true, entries: TOOLS });
    return result;
  }, [shell.data]);

  // Keyboard walks only what is visible: folded sections contribute their
  // header, not their entries.
  const visible = useMemo(
    () =>
      sections.flatMap((section) =>
        section.foldable && folded.includes(section.id) && !collapsed
          ? []
          : section.entries.map((entry) => ({ entry, section })),
      ),
    [collapsed, folded, sections],
  );

  const sidebarFocused = activePane === "sidebar";
  useShortcutScope("sidebar", sidebarFocused);
  const clamp = (index: number) => Math.max(0, Math.min(visible.length - 1, index));
  const current = visible[clamp(focusIndex)];
  useScopeController("sidebar", {
    down: () => setFocusIndex(clamp(focusIndex + 1)),
    up: () => setFocusIndex(clamp(focusIndex - 1)),
    top: () => setFocusIndex(0),
    bottom: () => setFocusIndex(visible.length - 1),
    open: () => {
      if (!current) return;
      void navigate({ to: current.entry.to });
      if (
        current.section.id === "mail" ||
        current.section.id === "labels" ||
        current.section.id === "saved"
      ) {
        setActivePane("mailbox");
      }
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
    const index = visible.findIndex(({ entry }) => isActive(path, entry.to));
    if (index >= 0 && index !== focusIndex) setFocusIndex(index);
  }, [focusIndex, path, setFocusIndex, sidebarFocused, visible]);

  useEffect(() => {
    if (!sidebarFocused) return;
    listRef.current
      ?.querySelector<HTMLElement>(`[data-nav-index="${clamp(focusIndex)}"]`)
      ?.scrollIntoView({ block: "nearest" });
  });

  let runningIndex = -1;
  return (
    <div className="flex h-full min-h-0 flex-col" onMouseDown={() => setActivePane("sidebar")}>
      <div className="border-b border-sidebar-border p-2">
        <AccountSwitcher collapsed={collapsed} />
      </div>

      <div
        ref={listRef}
        className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-2 py-2"
        data-active-pane={sidebarFocused ? "true" : undefined}
      >
        {sections.map((section) => {
          const isFolded = section.foldable && folded.includes(section.id) && !collapsed;
          return (
            <nav
              key={section.id}
              aria-label={section.title ?? "Mail"}
              className={cn(section.id !== "mail" && "mt-3")}
            >
              {section.title && !collapsed ? (
                <button
                  type="button"
                  onClick={() => toggleSection(section.id)}
                  aria-expanded={!isFolded}
                  className="group mb-0.5 flex w-full items-center gap-1 rounded px-2 py-1 font-mono text-[10.5px] uppercase tracking-[0.12em] text-faint hover:text-sidebar-foreground"
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
                    return (
                      <SidebarLink
                        key={entry.key}
                        entry={entry}
                        index={index}
                        collapsed={collapsed}
                        active={isActive(path, entry.to)}
                        focused={sidebarFocused && clamp(focusIndex) === index}
                      />
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
}: {
  entry: NavEntry;
  index: number;
  collapsed: boolean;
  active: boolean;
  focused: boolean;
}) {
  const count = entry.count && entry.count > 0 ? entry.count : null;
  const link = (
    <Link
      to={entry.to}
      data-nav-index={index}
      aria-current={active ? "page" : undefined}
      aria-label={collapsed ? `${entry.label}${count ? `, ${count} unread` : ""}` : undefined}
      className={cn(
        "group relative flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px] transition-colors",
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
      {!collapsed ? <span className="min-w-0 flex-1 truncate">{entry.label}</span> : null}
      {!collapsed && entry.shortcut ? (
        <span className="hidden font-mono text-2xs text-faint group-hover:inline">
          {formatChord(entry.shortcut)}
        </span>
      ) : null}
      {!collapsed && count ? (
        <span
          className={cn(
            "font-mono text-2xs tabular-nums group-hover:hidden",
            entry.emphasize
              ? "font-semibold text-sidebar-accent-foreground"
              : "text-muted-foreground",
          )}
        >
          {count > 9999 ? "9999+" : count.toLocaleString()}
        </span>
      ) : null}
      {collapsed && count && entry.emphasize ? (
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
