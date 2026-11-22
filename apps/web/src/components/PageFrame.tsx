import { MoreHorizontal, type LucideIcon } from "lucide-react";
import { createContext, useContext, useLayoutEffect, type ReactNode } from "react";
import { create } from "zustand";

import { ModeFrame, type FrameWidth } from "@/components/ModeFrame";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { usePublishPageTitle } from "@/state/pageTitleStore";
import { choiceIndex } from "@/lib/keys/choiceNavigation";
import { KeyChip } from "@/components/KeyChip";

export interface PageAction {
  label: string;
  onSelect: () => void;
  icon?: LucideIcon;
  destructive?: boolean;
  disabled?: boolean;
}

export interface FramePanes {
  split: boolean;
  inset: number;
}

const useListTabs = create<{ tabs: boolean }>(() => ({ tabs: false }));

export const FramePanesContext = createContext<FramePanes>({ split: false, inset: 0 });

export const READER_TOOLBAR_HEIGHT = 40;

export interface PageFrameProps {
  eyebrow?: ReactNode;
  title: string;
  titleContent?: ReactNode;
  purpose?: ReactNode;
  primary?: ReactNode;
  overflow?: readonly PageAction[];
  tabs?: ReactNode;
  tabsShortcut?: string;
  freshness?: ReactNode;
  width?: FrameWidth;
  wrapTitle?: boolean;
  detail?: boolean;
  publishTitle?: boolean;
  className?: string;
  titleTestId?: string;
  titleId?: string;
  purposeTestId?: string;
}

export function PageFrame({
  eyebrow,
  title,
  titleContent,
  purpose,
  primary,
  overflow,
  tabs,
  tabsShortcut,
  freshness,
  width = "list",
  wrapTitle: wrapWanted = false,
  detail = false,
  publishTitle = true,
  className,
  titleTestId,
  titleId,
  purposeTestId = "mode-header",
}: PageFrameProps) {
  usePublishPageTitle(publishTitle ? title : null);
  const panes = useContext(FramePanesContext);
  const listBesideDetail = panes.split && !detail;
  const wrapTitle = wrapWanted && !(panes.split && detail);
  const inset = listBesideDetail ? panes.inset : 0;
  const listHasTabs = useListTabs((s) => s.tabs);
  const registersTabs = listBesideDetail && Boolean(tabs);
  useLayoutEffect(() => {
    if (!registersTabs) return;
    useListTabs.setState({ tabs: true });
    return () => useListTabs.setState({ tabs: false });
  }, [registersTabs]);
  const matchTabbedList = panes.split && detail && listHasTabs && !tabs;
  const baseHeight = tabs ? 120 : 88;
  const hasActions = Boolean(primary) || (overflow?.length ?? 0) > 0;
  return (
    <header
      data-testid="page-frame"
      data-tabs={tabs ? "true" : "false"}
      className="shrink-0 border-b border-rule"
    >
      <ModeFrame
        width={width}
        style={inset ? { height: baseHeight + inset, paddingTop: 10 + inset } : undefined}
        className={cn(
          "flex flex-col px-5 pt-2.5",
          wrapTitle
            ? "min-h-16 pb-2.5 sm:min-h-[88px]"
            : tabs
              ? "h-24 sm:h-[120px]"
              : matchTabbedList
                ? "h-16 sm:h-[120px]"
                : "h-16 sm:h-[88px]",
          className,
        )}
      >
        <div className="type-eyebrow h-3.5 truncate text-text-3">{eyebrow}</div>
        <div
          className={cn(
            "flex shrink-0 items-center justify-between gap-4",
            wrapTitle ? "min-h-[34px]" : "h-[34px]",
          )}
        >
          <h1
            title={title}
            id={titleId}
            data-testid={titleTestId}
            className={cn(
              "type-title min-w-0 text-text-1",
              !listBesideDetail && "lg:type-display",
              wrapTitle ? "line-clamp-2 text-balance" : "truncate",
            )}
          >
            {titleContent ?? title}
          </h1>
          {hasActions ? (
            <div className="flex shrink-0 items-center gap-2">
              {primary}
              {overflow && overflow.length > 0 ? <PageOverflow actions={overflow} /> : null}
            </div>
          ) : null}
        </div>
        <div className="hidden h-5 shrink-0 items-center justify-between gap-4 sm:flex">
          <p data-testid={purposeTestId} className="type-body min-w-0 truncate text-text-2">
            {purpose}
          </p>
          {!tabs && freshness ? (
            <div className="type-meta shrink-0 text-text-3">{freshness}</div>
          ) : null}
        </div>
        {tabs ? (
          <div className="mt-auto flex h-8 shrink-0 items-end justify-between gap-4">
            <div
              className="min-w-0 flex-1 overflow-x-auto"
              data-choice-controls
              onKeyDown={(event) => {
                if (
                  event.defaultPrevented ||
                  event.altKey ||
                  event.ctrlKey ||
                  event.metaKey ||
                  event.shiftKey
                )
                  return;
                if (!(event.target instanceof Element)) return;
                const current = event.target.closest<HTMLAnchorElement>("a[href]");
                if (!current) return;
                const links = [
                  ...event.currentTarget.querySelectorAll<HTMLAnchorElement>("a[href]"),
                ];
                const next = choiceIndex(event.key, links.indexOf(current), links.length);
                if (next === null) return;
                event.preventDefault();
                links[next]?.focus();
              }}
            >
              {tabs}
            </div>
            <span
              className="shrink-0 pb-1"
              title="Arrow keys move between tabs; Home and End jump to the first and last. Enter opens a focused link."
            >
              {tabsShortcut ? <KeyChip className="mr-1">{tabsShortcut}</KeyChip> : null}
              <KeyChip>← →</KeyChip>
            </span>
            {freshness ? (
              <div className="type-meta hidden shrink-0 pb-2 text-text-3 sm:block">{freshness}</div>
            ) : null}
          </div>
        ) : null}
      </ModeFrame>
    </header>
  );
}

function PageOverflow({ actions }: { actions: readonly PageAction[] }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label="More actions"
          title="More actions"
          className="-mr-2 text-text-2"
        >
          <MoreHorizontal className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {actions.map((action) => (
          <DropdownMenuItem
            key={action.label}
            disabled={action.disabled}
            onSelect={action.onSelect}
            className={cn(action.destructive && "text-danger focus:text-danger")}
          >
            {action.icon ? <action.icon className="size-4" /> : null}
            {action.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
