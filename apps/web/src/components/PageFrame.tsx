import { MoreHorizontal, type LucideIcon } from "lucide-react";
import { createContext, useContext, type ReactNode } from "react";

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

/** One entry in a page's ⋯ menu. */
export interface PageAction {
  label: string;
  onSelect: () => void;
  icon?: LucideIcon;
  /** Destructive verbs are danger-coloured text, never filled (docs/blueprint/27). */
  destructive?: boolean;
  disabled?: boolean;
}

/**
 * Whether a list and a detail pane are side by side, so their headers can
 * line up. A list's frame then uses Title (not Display) and reserves `inset`
 * pixels for the detail pane's toolbar, which sits above the detail's frame;
 * the detail's frame keeps Display and stays on one line. `detail` on a frame
 * marks it as the detail pane's.
 */
export interface FramePanes {
  split: boolean;
  inset: number;
}

export const FramePanesContext = createContext<FramePanes>({ split: false, inset: 0 });

/** The height of a reader's toolbar row: what a list frame beside it reserves. */
export const READER_TOOLBAR_HEIGHT = 40;

export interface PageFrameProps {
  /** A short label above the title: the section, or a count ("1 due today"). */
  eyebrow?: ReactNode;
  /** The page's one title. The item's name on a detail page. */
  title: string;
  /** The title as drawn, for a heading that holds links; `title` stays its plain text. */
  titleContent?: ReactNode;
  /** One line saying what this page is for, 12 words or fewer. */
  purpose?: ReactNode;
  /** The page's one filled action. At most one (check-frames.mjs). */
  primary?: ReactNode;
  /** Everything else the page can do, in a ⋯ menu. */
  overflow?: readonly PageAction[];
  /** Underline tabs for the sections of this page. */
  tabs?: ReactNode;
  /** When this page's data was last true, in meta type. */
  freshness?: ReactNode;
  /** The column the header shares with the content under it. */
  width?: FrameWidth;
  /**
   * A detail pane's title (an email subject, an article headline) can run past
   * one line, and cutting it would hide what the person came to read: it wraps
   * to two lines and the frame grows to fit. Pages keep one line.
   */
  wrapTitle?: boolean;
  /** This is the detail pane's frame, beside a list's. */
  detail?: boolean;
  /** Hand the title to the Topbar so it drops a crumb that repeats it. */
  publishTitle?: boolean;
  className?: string;
  titleTestId?: string;
  titleId?: string;
  /** Defaults to "mode-header", which the mode pages' tests read. */
  purposeTestId?: string;
}

/**
 * The one header every page wears (docs/blueprint/27 "The page frame", D128):
 * eyebrow, title, a one-line purpose, at most one primary action and a ⋯
 * menu, then optional tabs. Its height is fixed (88px, or 120px with tabs) so
 * moving between pages never shifts the content, and nothing else goes in it.
 * The title is Display from 1024px and Title below. Under 640px the purpose
 * line and the freshness note go (the top bar carries freshness) and the
 * height drops to 64px, or 96px with tabs.
 */
export function PageFrame({
  eyebrow,
  title,
  titleContent,
  purpose,
  primary,
  overflow,
  tabs,
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
  // Beside a list, a wrapping title would push its rule off the list's.
  const wrapTitle = wrapWanted && !(panes.split && detail);
  const inset = listBesideDetail ? panes.inset : 0;
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
            <div className="min-w-0 flex-1 overflow-x-auto">{tabs}</div>
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
