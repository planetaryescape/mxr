import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import {
  Activity,
  Archive,
  Calendar,
  ChevronsLeft,
  ChevronsRight,
  Filter,
  History,
  Inbox,
  ListChecks,
  Mail,
  MessageSquareReply,
  Package,
  Search,
  Send,
  Settings,
  Shield,
  Sparkles,
  Star,
  Trash2,
  UserCog,
} from "lucide-react";
import { useEffect, useMemo, type ComponentType, type ReactNode } from "react";

import { AccountSwitcher } from "@/components/AccountSwitcher";
import { ThemePicker } from "@/components/ThemePicker";
import { ConnectionPill } from "@/components/ConnectionPill";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useShellQuery } from "@/features/mailbox/useMailboxQuery";
import type { SidebarItem } from "@/features/mailbox/types";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

interface NavItem {
  to: string;
  label: string;
  Icon: ComponentType<{ className?: string }>;
  badge?: string | number;
  shortcut?: string;
}

const primary: NavItem[] = [
  { to: "/m/inbox", label: "Mail", Icon: Mail, shortcut: "1" },
  { to: "/search", label: "Search", Icon: Search, shortcut: "2" },
  { to: "/analytics", label: "Analytics", Icon: Activity, shortcut: "3" },
  { to: "/rules", label: "Rules", Icon: Filter, shortcut: "4" },
  { to: "/screener", label: "Screener", Icon: Shield, shortcut: "5" },
  { to: "/subscriptions", label: "Subscriptions", Icon: Sparkles, shortcut: "6" },
  { to: "/reply-queue", label: "Reply queue", Icon: MessageSquareReply, shortcut: "7" },
  { to: "/invites", label: "Calendar invites", Icon: Calendar },
  { to: "/deliveries", label: "Deliveries", Icon: Package },
  { to: "/accounts", label: "Accounts", Icon: UserCog, shortcut: "8" },
];

const fallbackLenses: NavItem[] = [
  { to: "/m/inbox", label: "Inbox", Icon: Inbox },
  { to: "/m/starred", label: "Starred", Icon: Star },
  { to: "/m/snoozed", label: "Snoozed", Icon: Sparkles },
  { to: "/m/drafts", label: "Drafts", Icon: Mail },
  { to: "/m/sent", label: "Sent", Icon: Send },
  { to: "/m/archive", label: "Archive", Icon: Archive },
  { to: "/m/trash", label: "Trash", Icon: Trash2 },
];

const systemItems: NavItem[] = [
  { to: "/activity", label: "Activity log", Icon: History },
  { to: "/jobs", label: "Jobs", Icon: ListChecks },
  { to: "/diagnostics", label: "Diagnostics", Icon: Activity, shortcut: "9" },
  { to: "/settings/theme", label: "Settings", Icon: Settings, shortcut: "0" },
];

interface NavSection {
  label: string;
  items: NavItem[];
}

export function Sidebar() {
  const collapsed = useUiPrefs((s) => s.sidebarCollapsed);
  const setCollapsed = useUiPrefs((s) => s.setSidebarCollapsed);
  const navigate = useNavigate();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const shell = useShellQuery();
  const dynamicSections = shell.data?.sidebar?.sections;
  const activePane = useMailboxPane((state) => state.activePane);
  const setActivePane = useMailboxPane((state) => state.setActivePane);
  const sidebarIndex = useMailboxPane((state) => state.sidebarIndex);
  const setSidebarIndex = useMailboxPane((state) => state.setSidebarIndex);
  const sections = useMemo<NavSection[]>(() => {
    const lensSections =
      dynamicSections && dynamicSections.length > 0
        ? dynamicSections.map((section) => ({
            label: section.title,
            items: section.items.map((item) => ({
              to: sidebarItemPath(item),
              label: item.label,
              Icon: iconForSidebarItem(item),
              badge: item.unread && item.unread > 0 ? item.unread : undefined,
            })),
          }))
        : [{ label: "Lenses", items: fallbackLenses }];
    return [
      { label: "Workspace", items: primary },
      ...lensSections,
      { label: "System", items: systemItems },
    ];
  }, [dynamicSections]);
  const navigationItems = useMemo(() => sections.flatMap((section) => section.items), [sections]);

  useEffect(() => {
    const activeIndex = navigationItems.findIndex((item) => isItemActive(path, item.to));
    if (activeIndex >= 0 && activePane !== "sidebar" && sidebarIndex !== activeIndex) {
      setSidebarIndex(activeIndex);
    }
  }, [activePane, navigationItems, path, setSidebarIndex, sidebarIndex]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (activePane !== "sidebar") return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable=true]")) return;
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault();
        setSidebarIndex(Math.min(navigationItems.length - 1, sidebarIndex + 1));
      } else if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault();
        setSidebarIndex(Math.max(0, sidebarIndex - 1));
      } else if (
        event.key === "l" ||
        event.key === "ArrowRight" ||
        event.key === "Enter" ||
        event.key === "o"
      ) {
        event.preventDefault();
        const item = navigationItems[sidebarIndex];
        if (item) {
          void navigate({ to: item.to });
          setActivePane("mailbox");
        }
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activePane, navigate, navigationItems, setActivePane, setSidebarIndex, sidebarIndex]);

  let itemIndex = 0;

  return (
    <aside
      className="flex h-full flex-col bg-sidebar text-sidebar-foreground"
      aria-label="Mailbox sidebar"
    >
      <div className="border-b border-sidebar-border p-2">
        <AccountSwitcher collapsed={collapsed} />
      </div>

      <ScrollArea className="flex-1">
        <div className="px-2 py-3">
          {sections.map((section, sectionIndex) => (
            <SidebarSection
              key={section.label}
              label={section.label}
              collapsed={collapsed}
              className={sectionIndex > 0 ? "mt-4" : undefined}
            >
              {section.items.map((item) => {
                const index = itemIndex;
                itemIndex += 1;
                return (
                  <SidebarLink
                    key={`${section.label}-${item.to}-${item.label}`}
                    item={item}
                    collapsed={collapsed}
                    active={isItemActive(path, item.to)}
                    focused={activePane === "sidebar" && sidebarIndex === index}
                    onFocusPane={() => {
                      setSidebarIndex(index);
                      setActivePane("sidebar");
                    }}
                  />
                );
              })}
            </SidebarSection>
          ))}
        </div>
      </ScrollArea>

      <div
        className={cn(
          "border-t border-sidebar-border px-2 py-2",
          collapsed
            ? "flex flex-col items-center gap-1"
            : "flex items-center justify-between gap-2",
        )}
      >
        <ConnectionPill compact={collapsed} />
        <div className={cn("flex items-center gap-1", collapsed && "flex-col")}>
          <ThemePicker />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setCollapsed(!collapsed)}
                aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
              >
                {collapsed ? (
                  <ChevronsRight className="size-3.5" />
                ) : (
                  <ChevronsLeft className="size-3.5" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>{collapsed ? "Expand" : "Collapse"}</TooltipContent>
          </Tooltip>
        </div>
      </div>
    </aside>
  );
}

interface SectionProps {
  label: string;
  collapsed: boolean;
  className?: string;
  children: ReactNode;
}

function SidebarSection({ label, collapsed, className, children }: SectionProps) {
  return (
    <div className={className}>
      {!collapsed && (
        <div className="mb-1 px-2 text-2xs font-semibold uppercase tracking-wide text-sidebar-foreground/60">
          {label}
        </div>
      )}
      <nav className="flex flex-col gap-0.5">{children}</nav>
    </div>
  );
}

interface LinkProps {
  item: NavItem;
  collapsed: boolean;
  active: boolean;
  focused: boolean;
  onFocusPane: () => void;
}

function SidebarLink({ item, collapsed, active, focused, onFocusPane }: LinkProps) {
  const inner = (
    <Link
      to={item.to}
      data-focused={focused ? "true" : undefined}
      onFocus={onFocusPane}
      className={cn(
        "group flex items-center gap-2 rounded-md px-2 py-1.5 text-xs transition-colors",
        active
          ? "bg-sidebar-accent text-sidebar-accent-foreground"
          : "text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
        focused && "outline outline-1 outline-sidebar-ring/80",
      )}
      aria-current={active ? "page" : undefined}
    >
      <item.Icon className={cn("size-3.5 shrink-0", active && "text-sidebar-primary")} />
      {!collapsed && <span className="flex-1 truncate">{item.label}</span>}
      {!collapsed && item.badge !== undefined ? (
        <span
          className={cn(
            "font-mono text-2xs",
            active ? "text-sidebar-accent-foreground" : "text-sidebar-foreground/60",
          )}
        >
          {item.badge}
        </span>
      ) : null}
      {!collapsed && item.shortcut ? (
        <kbd className="rounded border border-sidebar-border bg-sidebar-accent px-1.5 py-0.5 font-mono text-2xs text-sidebar-foreground/60">
          {item.shortcut}
        </kbd>
      ) : null}
    </Link>
  );
  if (collapsed) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>{inner}</TooltipTrigger>
        <TooltipContent side="right">{item.label}</TooltipContent>
      </Tooltip>
    );
  }
  return inner;
}

function isItemActive(path: string, to: string): boolean {
  if (to === "/m/inbox") return path === to || path.startsWith(`${to}/`);
  return path === to || path.startsWith(`${to}/`);
}

function sidebarItemPath(item: SidebarItem): string {
  const lens = item.lens;
  if (!lens) return "/m/inbox";
  if (lens.kind === "inbox") return "/m/inbox";
  if (lens.kind === "all_mail") return "/m/archive";
  if (lens.kind === "saved_search") return `/m/saved/${item.id.replace(/^saved-search-/, "")}`;
  if (lens.kind === "label") return `/m/label/${item.id}`;
  if (lens.kind === "subscription") return `/m/label/${item.id}`;
  return "/m/inbox";
}

function iconForSidebarItem(item: SidebarItem): NavItem["Icon"] {
  const label = item.label.toLowerCase();
  if (label.includes("inbox")) return Inbox;
  if (label.includes("star")) return Star;
  if (label.includes("sent")) return Send;
  if (label.includes("draft")) return Mail;
  if (label.includes("spam")) return Shield;
  if (label.includes("trash")) return Trash2;
  if (label.includes("archive") || label.includes("all mail")) return Archive;
  if (label.includes("subscription")) return Sparkles;
  return Mail;
}
