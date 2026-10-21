import { Link, useRouterState } from "@tanstack/react-router";
import { Ellipsis, type LucideIcon } from "lucide-react";

import { CountBadge } from "@/components/CountBadge";
import { MODE_GLYPHS } from "@/components/ModeMark";
import { phoneTabFor, type PhoneTab } from "@/components/navMap";
import { useRailQuery } from "@/features/modes/rail";
import { cn } from "@/lib/utils";
import { useModals } from "@/state/modalStore";

interface Tab {
  key: PhoneTab;
  to: string;
  label: string;
  Icon: LucideIcon;
}

/**
 * The five phone tabs (Apple and Material both stop at five). The four
 * modes that are worked daily get a tab; More opens the sheet with every
 * other page, and is lit for all of them (D133, `phoneTabFor`).
 */
export const TABS: Tab[] = [
  { key: "now", to: "/now", label: "Now", Icon: MODE_GLYPHS.now },
  { key: "messages", to: "/messages", label: "Messages", Icon: MODE_GLYPHS.messages },
  { key: "todo", to: "/todo", label: "To do", Icon: MODE_GLYPHS.todo },
  { key: "reading", to: "/reading", label: "Reading", Icon: MODE_GLYPHS.reading },
  { key: "more", to: "", label: "More", Icon: Ellipsis },
];

const TAB_CLASS =
  "relative flex min-h-12 flex-col items-center justify-center gap-0.5 px-1 py-1.5 text-2xs";

export function MobileTabs() {
  const path = useRouterState({ select: (state) => state.location.pathname });
  const lane = useRouterState({
    select: (state) => ("turn" in state.location.search ? state.location.search.turn : undefined),
  });
  const badge = useRailQuery().data?.entries.find((entry) => entry.id === "now")?.badge ?? 0;
  const setMoreOpen = useModals((s) => s.setMoreSheetOpen);
  const lit = phoneTabFor(path, lane);
  return (
    <nav aria-label="Modes" className="app-shell-tabs" data-testid="mobile-tabs">
      {TABS.map((tab) => {
        const active = tab.key === lit;
        const className = cn(TAB_CLASS, active ? "text-text-1" : "text-text-3");
        const body = (
          <>
            <tab.Icon aria-hidden className="size-5" />
            <span className="truncate">{tab.label}</span>
            {active ? (
              <span aria-hidden className="absolute inset-x-3 top-0 h-0.5 bg-signal" />
            ) : null}
          </>
        );
        if (tab.key === "more") {
          return (
            <button
              key={tab.key}
              type="button"
              data-more-trigger
              aria-haspopup="dialog"
              aria-current={active ? "page" : undefined}
              onClick={() => setMoreOpen(true)}
              className={className}
            >
              {body}
            </button>
          );
        }
        return (
          <Link
            key={tab.key}
            to={tab.to}
            activeOptions={{ exact: true }}
            aria-current={active ? "page" : undefined}
            className={className}
          >
            {body}
            {tab.key === "now" && badge > 0 ? (
              <CountBadge className="absolute right-[calc(50%-1.25rem)] top-1">
                <span className="sr-only">, </span>
                {badge}
              </CountBadge>
            ) : null}
          </Link>
        );
      })}
    </nav>
  );
}
