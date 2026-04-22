import { RefreshCw } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type {
  FocusContext,
  SidebarItem,
  SidebarPayload,
  WorkbenchScreen,
} from "../../shared/types";
import type { ConnectionStatus } from "../state/useEventStream";
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
      <div
        className="flex size-7 items-center justify-center border border-outline bg-canvas-elevated text-[length:var(--text-xs)] font-semibold text-foreground"
        style={{ borderRadius: "var(--radius-sm)" }}
      >
        mxr
      </div>
      <div className="flex min-w-0 items-center gap-0.5">
        {props.screens.map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={item.id}
              type="button"
              aria-label={item.label}
              className={cn(
                "flex h-7 items-center gap-1.5 border-b-2 px-2 text-[length:var(--text-xs)] uppercase text-foreground-subtle transition-colors",
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
  selectedItemId: string | null;
  focused: boolean;
  accountLabel: string;
  accounts: Array<{ key: string; name: string; is_default: boolean }>;
  onSelectSidebarItem: (itemId: string) => void;
  onSwitchAccount: (key: string) => void;
  onApplySidebarLens: (item: SidebarItem) => void;
}) {
  const [accountPickerOpen, setAccountPickerOpen] = useState(false);
  const pickerRef = useRef<HTMLDivElement>(null);

  // Close picker on outside click
  useEffect(() => {
    if (!accountPickerOpen) return;
    const handler = (e: MouseEvent) => {
      if (pickerRef.current && !pickerRef.current.contains(e.target as Node)) {
        setAccountPickerOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [accountPickerOpen]);

  return (
    <aside className="surface subtle-scrollbar hidden w-48 shrink-0 overflow-y-auto border-y-0 border-l-0 bg-panel-muted px-3 py-3 md:block">
      {/* Account switcher */}
      <div className="relative" ref={pickerRef}>
        <button
          type="button"
          className="flex w-full items-center justify-between gap-2"
          onClick={() => setAccountPickerOpen(!accountPickerOpen)}
        >
          <span className="truncate text-[length:var(--text-sm)] font-semibold text-foreground">
            {props.accountLabel}
          </span>
          {props.unreadCount > 0 ? (
            <span
              className="flex h-5 min-w-5 items-center justify-center bg-accent/15 px-1.5 font-mono text-[length:var(--text-xs)] font-medium tabular-nums text-accent"
              style={{ borderRadius: "var(--radius-sm)" }}
            >
              {props.unreadCount}
            </span>
          ) : null}
        </button>

        {/* Account picker dropdown */}
        {accountPickerOpen && props.accounts.length > 0 ? (
          <AccountPicker
            accounts={props.accounts}
            onSelect={(key) => {
              props.onSwitchAccount(key);
              setAccountPickerOpen(false);
            }}
            onClose={() => setAccountPickerOpen(false)}
          />
        ) : null}
      </div>

      <div className="mt-4 space-y-3.5">
        {props.sidebar.sections.map((section) => (
          <section key={section.id}>
            <p className="mono-meta">{section.title}</p>
            <div className="mt-1.5 space-y-px">
              {section.items.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={cn(
                    "flex w-full items-center justify-between border-l-2 px-2 py-1.5 text-left text-[length:var(--text-sm)] transition-colors",
                    props.focused && props.selectedItemId === item.id
                      ? "border-l-accent bg-panel-elevated text-foreground"
                      : item.active
                        ? "border-l-accent/40 bg-panel-elevated/60 text-foreground"
                        : "border-l-transparent text-foreground-muted hover:bg-panel-elevated/40 hover:text-foreground",
                  )}
                  onClick={() => props.onApplySidebarLens(item)}
                  onMouseEnter={() => props.onSelectSidebarItem(item.id)}
                >
                  <span className="truncate">{item.label}</span>
                  {item.unread > 0 ? (
                    <span className="font-mono text-[length:var(--text-xs)] font-medium tabular-nums text-accent">
                      {item.unread}
                    </span>
                  ) : (
                    <span className="font-mono text-[length:var(--text-2xs)] tabular-nums text-foreground-subtle">
                      {item.total}
                    </span>
                  )}
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
  pendingMailboxLabel: string | null;
  pendingMutationLabel: string | null;
  canResumeDraft: boolean;
  onResumeDraft: () => void;
  onSync: () => void;
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
        <p className="truncate text-[length:var(--text-sm)] text-foreground-muted">{props.statusMessage}</p>
        {props.pendingBindingTokens ? (
          <span className="font-mono text-[length:var(--text-xs)] tabular-nums text-warning">
            {props.pendingBindingTokens.join("")}
          </span>
        ) : null}
        {props.actionNotice ? (
          <span
            className="border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-[length:var(--text-xs)] text-warning"
            style={{ borderRadius: "var(--radius-sm)" }}
          >
            {props.actionNotice}
          </span>
        ) : null}
        {props.pendingMailboxLabel ? (
          <span
            role="status"
            aria-live="polite"
            className="inline-flex items-center gap-1 border border-accent/30 bg-accent/10 px-1.5 py-0.5 text-[length:var(--text-xs)] text-accent"
            style={{ borderRadius: "var(--radius-sm)" }}
          >
            <RefreshCw className="size-3 animate-spin" />
            <span>{`Loading ${props.pendingMailboxLabel}...`}</span>
          </span>
        ) : null}
        {props.pendingMutationLabel ? (
          <span
            aria-live="polite"
            className="border border-accent/30 bg-accent/10 px-1.5 py-0.5 text-[length:var(--text-xs)] text-accent"
            style={{ borderRadius: "var(--radius-sm)" }}
          >
            {props.pendingMutationLabel}
          </span>
        ) : null}
        {props.canResumeDraft ? (
          <button
            type="button"
            className="border border-accent/30 bg-accent/12 px-1.5 py-0.5 text-[length:var(--text-xs)] text-accent"
            style={{ borderRadius: "var(--radius-sm)" }}
            onClick={props.onResumeDraft}
          >
            Resume draft
          </button>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <div className="hidden items-center gap-1 xl:flex">
          <button
            type="button"
            aria-label="Sync now"
            className="flex size-6 items-center justify-center border border-outline bg-canvas-elevated text-foreground-subtle transition-colors hover:border-outline-strong hover:text-foreground"
            style={{ borderRadius: "var(--radius-sm)" }}
            onClick={props.onSync}
          >
            <RefreshCw className="size-3" />
          </button>
          <HeaderActionButton label="Compose" shortcut="C" onClick={props.onCompose} />
          <HeaderActionButton
            label="Reply"
            shortcut="R"
            disabled={!props.selectedRowAvailable}
            onClick={props.onReply}
          />
          <HeaderActionButton
            label="Forward"
            shortcut="F"
            disabled={!props.selectedRowAvailable}
            onClick={props.onForward}
          />
          <HeaderActionButton
            label="Label"
            shortcut="L"
            disabled={!props.selectedRowAvailable}
            onClick={props.onLabel}
          />
          <HeaderActionButton
            label="Snooze"
            shortcut="Z"
            disabled={!props.selectedRowAvailable}
            onClick={props.onSnooze}
          />
        </div>
        <div
          className="flex items-center gap-1.5 border border-outline bg-canvas-elevated px-2 py-1 text-[length:var(--text-xs)] text-foreground-subtle"
          style={{ borderRadius: "var(--radius-sm)" }}
        >
          <span className="font-mono uppercase tabular-nums">{props.accountLabel}</span>
          <span className="size-1.5 rounded-full bg-success" />
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
  eventStreamStatus?: ConnectionStatus;
  commandHint: string;
  totalThreads: number;
}) {
  return (
    <footer className="surface flex h-6 shrink-0 items-center justify-between border-x-0 border-b-0 px-3">
      <div className="flex min-w-0 items-center gap-2.5 overflow-hidden text-[length:var(--text-2xs)] text-foreground-subtle">
        {props.hints.map((hint) => (
          <span key={`${hint.key}-${hint.label}`} className="truncate">
            <kbd className="font-mono text-foreground-muted">{hint.key}</kbd>
            <span className="ml-0.5">{hint.label}</span>
          </span>
        ))}
      </div>
      <div className="flex items-center gap-2 text-[length:var(--text-2xs)] text-foreground-subtle tabular-nums">
        {props.eventStreamStatus ? (
          <span className="flex items-center gap-1">
            <span
              className={cn(
                "size-1.5 rounded-full",
                props.eventStreamStatus === "connected" && "bg-success",
                props.eventStreamStatus === "connecting" && "bg-warning",
                props.eventStreamStatus === "disconnected" && "bg-danger",
              )}
            />
            <span className="font-mono uppercase">{props.eventStreamStatus === "connected" ? "live" : props.eventStreamStatus}</span>
          </span>
        ) : null}
        <span className="font-mono uppercase">{props.screen}</span>
        <span className="font-mono uppercase">{props.layoutMode}</span>
        <span className="font-mono uppercase">{props.focusContext}</span>
        <span>{props.commandHint}</span>
        <span>{props.totalThreads} cached</span>
      </div>
    </footer>
  );
}

function AccountPicker(props: {
  accounts: Array<{ key: string; name: string; is_default: boolean }>;
  onSelect: (key: string) => void;
  onClose: () => void;
}) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listRef.current?.focus();
  }, []);

  return (
    <div
      ref={listRef}
      tabIndex={-1}
      className="absolute left-0 right-0 top-full z-20 mt-1 border border-outline bg-panel shadow-xl outline-none"
      style={{ borderRadius: "var(--radius-sm)" }}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown" || e.key === "j") {
          e.preventDefault();
          setSelectedIndex((i) => Math.min(i + 1, props.accounts.length - 1));
        } else if (e.key === "ArrowUp" || e.key === "k") {
          e.preventDefault();
          setSelectedIndex((i) => Math.max(i - 1, 0));
        } else if (e.key === "Enter") {
          e.preventDefault();
          const account = props.accounts[selectedIndex];
          if (account) props.onSelect(account.key);
        } else if (e.key === "Escape") {
          e.preventDefault();
          props.onClose();
        }
      }}
    >
      <div className="px-2 py-1.5">
        <p className="mono-meta">Switch account</p>
      </div>
      {props.accounts.map((account, i) => (
        <button
          key={account.key}
          type="button"
          className={cn(
            "flex w-full items-center justify-between px-3 py-1.5 text-left text-[length:var(--text-sm)] transition-colors",
            i === selectedIndex
              ? "border-l-2 border-l-accent bg-panel-elevated text-foreground"
              : "border-l-2 border-l-transparent text-foreground-muted hover:bg-panel-elevated/40",
          )}
          onClick={() => props.onSelect(account.key)}
          onMouseEnter={() => setSelectedIndex(i)}
        >
          <span className="truncate">{account.name}</span>
          {account.is_default ? (
            <span className="text-[length:var(--text-xs)] text-accent">active</span>
          ) : null}
        </button>
      ))}
      <div className="border-t border-outline/50 px-3 py-1.5 text-[length:var(--text-2xs)] text-foreground-subtle">
        <kbd className="font-mono text-foreground-muted">j/k</kbd> navigate
        <span className="mx-1">·</span>
        <kbd className="font-mono text-foreground-muted">Enter</kbd> switch
      </div>
    </div>
  );
}
