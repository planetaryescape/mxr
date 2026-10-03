import { Link, useRouterState } from "@tanstack/react-router";
import { ListTodo, MessagesSquare, Newspaper, Search, Sun, type LucideIcon } from "lucide-react";

import { useRailQuery } from "@/features/modes/rail";
import { cn } from "@/lib/utils";

interface Tab {
  key: string;
  to: string;
  label: string;
  Icon: LucideIcon;
  /** Paths that count as this tab. */
  owns: (path: string) => boolean;
}

const under = (base: string) => (path: string) => path === base || path.startsWith(`${base}/`);

/**
 * The five phone tabs (Apple and Material both stop at five). Find holds
 * Archive, search and Inbox, since Archive is a search; Updates opens from
 * Now's card.
 */
export const TABS: Tab[] = [
  { key: "now", to: "/now", label: "Now", Icon: Sun, owns: under("/now") },
  {
    key: "messages",
    to: "/messages",
    label: "Messages",
    Icon: MessagesSquare,
    owns: under("/messages"),
  },
  { key: "todo", to: "/todo", label: "To do", Icon: ListTodo, owns: under("/todo") },
  { key: "reading", to: "/reading", label: "Reading", Icon: Newspaper, owns: under("/reading") },
  {
    key: "find",
    to: "/find",
    label: "Find",
    Icon: Search,
    owns: (path) => ["/find", "/archive", "/search", "/m"].some((base) => under(base)(path)),
  },
];

export function MobileTabs() {
  const path = useRouterState({ select: (state) => state.location.pathname });
  const badge = useRailQuery().data?.entries.find((entry) => entry.id === "now")?.badge ?? 0;
  return (
    <nav aria-label="Modes" className="app-shell-tabs" data-testid="mobile-tabs">
      {TABS.map((tab) => {
        const active = tab.owns(path);
        return (
          <Link
            key={tab.key}
            to={tab.to}
            aria-current={active ? "page" : undefined}
            className={cn(
              "relative flex min-h-12 flex-col items-center justify-center gap-0.5 px-1 py-1.5 text-[11px]",
              active ? "text-sidebar-primary" : "text-muted-foreground",
            )}
          >
            <tab.Icon aria-hidden className="size-5" />
            <span className="truncate">{tab.label}</span>
            {tab.key === "now" && badge > 0 ? (
              <span className="absolute right-[calc(50%-1.25rem)] top-1 rounded-full bg-primary px-1 font-mono text-[9.5px] leading-4 text-primary-foreground tabular-nums">
                <span className="sr-only">, </span>
                {badge}
              </span>
            ) : null}
          </Link>
        );
      })}
    </nav>
  );
}
