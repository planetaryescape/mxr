import { Search } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type {
  FocusContext,
  SidebarItem,
  SidebarPayload,
  WorkbenchScreen,
} from "../../shared/types";
import { cn } from "../lib/cn";
import { HeaderActionButton } from "./shared";

export function ActivityRail(props: {
  screen: WorkbenchScreen;
  screens: Array<{
    id: WorkbenchScreen;
    label: string;
    icon: LucideIcon;
    accent: string;
  }>;
  commandHint: string;
  onSwitch: (screen: WorkbenchScreen) => void;
}) {
  return (
    <aside className="surface flex w-12 shrink-0 flex-col items-center justify-between border-y-0 border-l-0 bg-panel-muted px-1.5 py-2.5">
      <div className="flex w-full flex-col items-center gap-2">
        <div className="mx-auto flex size-8 items-center justify-center rounded-xl border border-outline bg-canvas-elevated text-[10px] font-semibold text-foreground">
          mxr
        </div>
        {props.screens.map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={item.id}
              aria-label={item.label}
              title={item.label}
              className={cn(
                "relative flex size-9 items-center justify-center rounded-lg transition-colors",
                props.screen === item.id
                  ? "bg-panel-elevated text-foreground"
                  : "text-foreground-subtle hover:bg-panel hover:text-foreground",
              )}
              onClick={() => props.onSwitch(item.id)}
            >
              {props.screen === item.id ? (
                <span className="absolute inset-y-2 left-0 w-px rounded-full bg-accent" />
              ) : null}
              <Icon
                className={cn("size-4 shrink-0", props.screen === item.id ? item.accent : "")}
              />
              <span className="sr-only">{item.label}</span>
            </button>
          );
        })}
      </div>
      <div className="flex w-full flex-col gap-2">
        <div className="rounded-xl border border-outline bg-canvas-elevated px-1.5 py-1.5 text-center">
          <p className="text-[10px] text-foreground-subtle">⌘P</p>
        </div>
      </div>
    </aside>
  );
}

export function NavigationSidebar(props: {
  unreadCount: number;
  searchQuery: string;
  onSearchQueryChange: (value: string) => void;
  onRunSearch: () => void;
  sidebar: SidebarPayload;
  onApplySidebarLens: (item: SidebarItem) => void;
}) {
  return (
    <aside className="surface subtle-scrollbar hidden w-64 shrink-0 overflow-y-auto border-y-0 border-l-0 bg-panel px-4 py-4 md:block">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="mono-meta">Workspace</p>
          <h2 className="mt-2 text-balance text-[1.4rem] font-semibold leading-none text-foreground">
            Mailroom
          </h2>
        </div>
        <div className="rounded-xl border border-outline bg-canvas-elevated px-2.5 py-2 text-right tabular-nums">
          <p className="text-[10px] uppercase text-foreground-subtle">Unread</p>
          <p className="mt-1 text-[13px] font-medium text-foreground">{props.unreadCount}</p>
        </div>
      </div>

      <div className="mt-5 flex gap-2">
        <div className="relative min-w-0 flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-foreground-subtle" />
          <input
            className="min-w-0 w-full rounded-xl border border-outline bg-canvas-elevated py-2 pl-8.5 pr-3 text-[13px] text-foreground outline-none placeholder:text-foreground-subtle focus:border-outline-strong"
            aria-label="Search"
            value={props.searchQuery}
            onChange={(event) => props.onSearchQueryChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                props.onRunSearch();
              }
            }}
            placeholder="Search local mail"
          />
        </div>
        <button
          type="button"
          aria-label="Run search"
          className="rounded-xl border border-outline bg-canvas-elevated px-2.5 py-2 text-[13px] text-foreground-muted transition-colors hover:border-outline-strong hover:text-foreground"
          onClick={props.onRunSearch}
        >
          Go
        </button>
      </div>

      <div className="mt-6 space-y-5">
        {props.sidebar.sections.map((section) => (
          <section key={section.id}>
            <p className="mono-meta">{section.title}</p>
            <div className="mt-2.5 space-y-0.5">
              {section.items.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={cn(
                    "flex w-full items-center justify-between rounded-lg px-2.5 py-2 text-left text-[13px] transition-colors",
                    item.active
                      ? "border border-outline-strong bg-panel-elevated text-foreground"
                      : "border border-transparent text-foreground-muted hover:bg-panel-elevated/70 hover:text-foreground",
                  )}
                  onClick={() => props.onApplySidebarLens(item)}
                >
                  <span className="truncate">{item.label}</span>
                  <span className="font-mono text-[10px] tabular-nums text-foreground-subtle">
                    {item.unread}/{item.total}
                  </span>
                </button>
              ))}
            </div>
          </section>
        ))}
      </div>
    </aside>
  );
}

export function WorkbenchHeader(props: {
  statusMessage: string;
  pendingBindingTokens: string[] | null;
  actionNotice: string | null;
  pendingMutationLabel: string | null;
  canResumeDraft: boolean;
  onResumeDraft: () => void;
  onCompose: () => void;
  onReply: () => void;
  onForward: () => void;
  onLabel: () => void;
  onSnooze: () => void;
  selectedRowAvailable: boolean;
  accountLabel: string;
  syncLabel: string;
}) {
  return (
    <header className="surface flex h-11 shrink-0 items-center justify-between border-x-0 border-t-0 bg-panel px-3">
      <div className="flex min-w-0 items-center gap-2.5">
        <div className="rounded-md border border-outline bg-canvas-elevated px-2 py-1">
          <span className="text-[10px] uppercase text-foreground-subtle">mail</span>
        </div>
        <p className="truncate text-[13px] text-foreground-muted">{props.statusMessage}</p>
        {props.pendingBindingTokens ? (
          <span className="font-mono text-[11px] tabular-nums text-warning">
            {props.pendingBindingTokens.join("")}
          </span>
        ) : null}
        {props.actionNotice ? (
          <span className="rounded-full border border-warning/30 bg-warning/10 px-2 py-0.5 text-[11px] text-warning">
            {props.actionNotice}
          </span>
        ) : null}
        {props.pendingMutationLabel ? (
          <span
            aria-live="polite"
            className="rounded-full border border-accent/30 bg-accent/10 px-2 py-0.5 text-[11px] text-accent"
          >
            {props.pendingMutationLabel}
          </span>
        ) : null}
        {props.canResumeDraft ? (
          <button
            type="button"
            className="rounded-full border border-accent/30 bg-accent/12 px-2 py-0.5 text-[11px] text-accent"
            onClick={props.onResumeDraft}
          >
            Resume draft
          </button>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-2.5">
        <div className="hidden items-center gap-1.5 xl:flex">
          <HeaderActionButton label="Compose" onClick={props.onCompose} />
          <HeaderActionButton
            label="Reply"
            disabled={!props.selectedRowAvailable}
            onClick={props.onReply}
          />
          <HeaderActionButton
            label="Forward"
            disabled={!props.selectedRowAvailable}
            onClick={props.onForward}
          />
          <HeaderActionButton
            label="Label"
            disabled={!props.selectedRowAvailable}
            onClick={props.onLabel}
          />
          <HeaderActionButton
            label="Snooze"
            disabled={!props.selectedRowAvailable}
            onClick={props.onSnooze}
          />
        </div>
        <div className="flex items-center gap-2 rounded-full border border-outline bg-canvas-elevated px-2.5 py-1 text-[11px] text-foreground-subtle">
          <span className="font-mono uppercase tabular-nums">{props.accountLabel}</span>
          <span className="h-1.5 w-1.5 rounded-full bg-success" />
          <span>{props.syncLabel}</span>
        </div>
      </div>
    </header>
  );
}

export function WorkbenchStatusBar(props: {
  screen: WorkbenchScreen;
  layoutMode: string;
  focusContext: FocusContext;
  commandHint: string;
  totalThreads: number;
}) {
  return (
    <footer className="surface flex h-7 shrink-0 items-center justify-between border-x-0 border-b-0 px-3">
      <div className="flex items-center gap-3 text-[10px] text-foreground-subtle">
        <span className="font-mono uppercase tabular-nums">{props.screen}</span>
        <span className="font-mono uppercase tabular-nums">{props.layoutMode}</span>
        <span className="font-mono uppercase tabular-nums">{props.focusContext}</span>
      </div>
      <div className="flex items-center gap-3 text-[10px] text-foreground-subtle tabular-nums">
        <span>{props.commandHint} command palette</span>
        <span>{props.totalThreads} threads cached</span>
      </div>
    </footer>
  );
}
