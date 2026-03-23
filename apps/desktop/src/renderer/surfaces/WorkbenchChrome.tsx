import type { LucideIcon } from "lucide-react";
import type {
  FocusContext,
  SidebarItem,
  SidebarPayload,
  WorkbenchScreen,
} from "../../shared/types";
import { cn } from "../lib/cn";
import { HeaderActionButton } from "./shared";

export function WorkbenchTabs(props: {
  screen: WorkbenchScreen;
  screens: Array<{
    id: WorkbenchScreen;
    label: string;
    icon: LucideIcon;
    accent: string;
  }>;
  onSwitch: (screen: WorkbenchScreen) => void;
}) {
  return (
    <nav className="flex min-w-0 items-center gap-2">
      <div className="flex size-7 items-center justify-center border border-outline bg-canvas-elevated text-[10px] font-semibold text-foreground">
        mxr
      </div>
      <div className="flex min-w-0 items-center gap-1">
        {props.screens.map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={item.id}
              type="button"
              aria-label={item.label}
              className={cn(
                "flex h-7 items-center gap-1.5 border-b px-2 text-[11px] uppercase text-foreground-subtle transition-colors",
                props.screen === item.id
                  ? "border-accent text-foreground"
                  : "border-transparent hover:border-outline-strong hover:text-foreground",
              )}
              onClick={() => props.onSwitch(item.id)}
            >
              <Icon
                className={cn("size-3.5 shrink-0", props.screen === item.id ? item.accent : "")}
              />
              <span>{item.label}</span>
            </button>
          );
        })}
      </div>
    </nav>
  );
}

export function NavigationSidebar(props: {
  unreadCount: number;
  sidebar: SidebarPayload;
  onApplySidebarLens: (item: SidebarItem) => void;
}) {
  return (
    <aside className="surface subtle-scrollbar hidden w-48 shrink-0 overflow-y-auto border-y-0 border-l-0 bg-panel px-3 py-3 md:block">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="mono-meta">Workspace</p>
          <h2 className="mt-1 text-balance text-[1.2rem] font-semibold leading-none text-foreground">
            Mailroom
          </h2>
        </div>
        <div className="border border-outline bg-canvas-elevated px-2 py-1.5 text-right tabular-nums">
          <p className="text-[9px] uppercase text-foreground-subtle">Unread</p>
          <p className="mt-0.5 text-[12px] font-medium text-foreground">{props.unreadCount}</p>
        </div>
      </div>

      <div className="mt-5 space-y-4">
        {props.sidebar.sections.map((section) => (
          <section key={section.id}>
            <p className="mono-meta">{section.title}</p>
            <div className="mt-1.5 space-y-px">
              {section.items.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={cn(
                    "flex w-full items-center justify-between border border-transparent px-2 py-1.5 text-left text-[12px] transition-colors",
                    item.active
                      ? "border-outline-strong bg-panel-elevated text-foreground"
                      : "text-foreground-muted hover:bg-panel-elevated/55 hover:text-foreground",
                  )}
                  onClick={() => props.onApplySidebarLens(item)}
                >
                  <span className="truncate">{item.label}</span>
                  <span className="font-mono text-[9px] tabular-nums text-foreground-subtle">
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
  screen: WorkbenchScreen;
  screens: Array<{
    id: WorkbenchScreen;
    label: string;
    icon: LucideIcon;
    accent: string;
  }>;
  onSwitch: (screen: WorkbenchScreen) => void;
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
    <header className="surface flex h-10 shrink-0 items-center justify-between border-x-0 border-t-0 bg-panel px-3">
      <div className="flex min-w-0 items-center gap-3">
        <WorkbenchTabs screen={props.screen} screens={props.screens} onSwitch={props.onSwitch} />
        <p className="truncate text-[12px] text-foreground-muted">{props.statusMessage}</p>
        {props.pendingBindingTokens ? (
          <span className="font-mono text-[10px] tabular-nums text-warning">
            {props.pendingBindingTokens.join("")}
          </span>
        ) : null}
        {props.actionNotice ? (
          <span className="border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-[10px] text-warning">
            {props.actionNotice}
          </span>
        ) : null}
        {props.pendingMutationLabel ? (
          <span
            aria-live="polite"
            className="border border-accent/30 bg-accent/10 px-1.5 py-0.5 text-[10px] text-accent"
          >
            {props.pendingMutationLabel}
          </span>
        ) : null}
        {props.canResumeDraft ? (
          <button
            type="button"
            className="border border-accent/30 bg-accent/12 px-1.5 py-0.5 text-[10px] text-accent"
            onClick={props.onResumeDraft}
          >
            Resume draft
          </button>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <div className="hidden items-center gap-1 xl:flex">
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
        <div className="flex items-center gap-1.5 border border-outline bg-canvas-elevated px-2 py-1 text-[10px] text-foreground-subtle">
          <span className="font-mono uppercase tabular-nums">{props.accountLabel}</span>
          <span className="size-1 rounded-full bg-success" />
          <span>{props.syncLabel}</span>
        </div>
      </div>
    </header>
  );
}

export function WorkbenchStatusBar(props: {
  hints: Array<{ key: string; label: string }>;
  screen: WorkbenchScreen;
  layoutMode: string;
  focusContext: FocusContext;
  commandHint: string;
  totalThreads: number;
}) {
  return (
    <footer className="surface flex h-6 shrink-0 items-center justify-between border-x-0 border-b-0 px-3">
      <div className="flex min-w-0 items-center gap-2 overflow-hidden text-[9px] text-foreground-subtle">
        {props.hints.map((hint) => (
          <span key={`${hint.key}-${hint.label}`} className="truncate">
            <span className="font-mono text-foreground">{hint.key}</span>
            <span>:{hint.label}</span>
          </span>
        ))}
      </div>
      <div className="flex items-center gap-2 text-[9px] text-foreground-subtle tabular-nums">
        <span className="font-mono uppercase">{props.screen}</span>
        <span className="font-mono uppercase">{props.layoutMode}</span>
        <span className="font-mono uppercase">{props.focusContext}</span>
        <span>{props.commandHint}</span>
        <span>{props.totalThreads} threads cached</span>
      </div>
    </footer>
  );
}
