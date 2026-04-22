import { AlertTriangle, Command } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import type { BridgeState } from "../../shared/types";
import { cn } from "../lib/cn";
import { HeaderActionButton, StatCard } from "./shared";

export function BridgeFrame({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-dvh items-center justify-center bg-canvas px-6 py-10 text-foreground">
      {children}
    </div>
  );
}

export function BridgeMismatchView(props: {
  bridge: Extract<BridgeState, { kind: "mismatch" }>;
  externalPath: string;
  onExternalPathChange: (value: string) => void;
  onUseBundled: () => void;
  onRetry: () => void;
  onTryExternal: () => void;
}) {
  return (
    <BridgeFrame>
      <section className="surface mx-auto flex w-full max-w-4xl flex-col gap-4 px-5 py-5">
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="mono-meta">mxr Desktop</p>
            <h1 className="mt-2 text-2xl font-semibold tracking-tight text-foreground">
              mxr Desktop needs a compatible version of mxr
            </h1>
            <p className="mt-3 max-w-2xl text-sm leading-6 text-foreground-muted">
              {props.bridge.detail}
            </p>
          </div>
          <AlertTriangle className="mt-2 size-8 text-warning" />
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          <StatCard label="Found daemon version" value={props.bridge.daemonVersion ?? "unknown"} />
          <StatCard
            label="Found protocol"
            value={String(props.bridge.actualProtocol ?? "unknown")}
          />
          <StatCard label="Required protocol" value={String(props.bridge.requiredProtocol)} />
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            className="border border-accent/30 bg-accent/15 px-3 py-1.5 text-xs font-medium uppercase text-accent"
            onClick={props.onUseBundled}
          >
            Use bundled mxr
          </button>
          <button
            className="border border-outline bg-panel-elevated px-3 py-1.5 text-xs uppercase text-foreground-muted"
            onClick={props.onRetry}
          >
            Retry
          </button>
        </div>
        <div className="surface-muted px-4 py-4">
          <p className="mono-meta">Update steps</p>
          <ul className="mt-4 space-y-3 text-sm text-foreground-muted">
            {props.bridge.updateSteps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ul>
        </div>
        <div className="surface-muted grid gap-2 px-4 py-4">
          <label className="mono-meta" htmlFor="external-binary">
            Advanced external mxr binary
          </label>
          <input
            id="external-binary"
            className="border border-outline bg-canvas-elevated px-3 py-2 text-sm text-foreground outline-none ring-0 placeholder:text-foreground-subtle"
            value={props.externalPath}
            onChange={(event) => props.onExternalPathChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                props.onTryExternal();
              }
            }}
            placeholder="/usr/local/bin/mxr"
          />
          <button
            className="w-fit border border-outline bg-panel-elevated px-3 py-1.5 text-xs uppercase text-foreground-muted"
            onClick={props.onTryExternal}
          >
            Try external binary
          </button>
        </div>
      </section>
    </BridgeFrame>
  );
}

export function BridgeErrorView(props: {
  title: string;
  detail: string;
  updateSteps: string[];
  onRetry: () => void;
}) {
  return (
    <BridgeFrame>
      <section className="surface mx-auto flex w-full max-w-3xl flex-col gap-4 px-5 py-5">
        <p className="mono-meta">mxr Desktop</p>
        <h1 className="text-2xl font-semibold tracking-tight text-foreground">{props.title}</h1>
        <p className="max-w-2xl text-sm leading-6 text-foreground-muted">{props.detail}</p>
        <div className="surface-muted px-4 py-4">
          <p className="mono-meta">Useful next steps</p>
          <ul className="mt-4 space-y-3 text-sm text-foreground-muted">
            {props.updateSteps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ul>
        </div>
        <button
          className="w-fit border border-outline bg-panel-elevated px-3 py-1.5 text-xs uppercase text-foreground"
          onClick={props.onRetry}
        >
          Retry
        </button>
      </section>
    </BridgeFrame>
  );
}

export function BridgeLoadingView(props: { title: string; detail: string }) {
  return (
    <BridgeFrame>
      <section className="surface mx-auto flex w-full max-w-xl flex-col gap-3 px-5 py-5">
        <p className="mono-meta">mxr Desktop</p>
        <h1 className="text-2xl font-semibold tracking-tight text-foreground">{props.title}</h1>
        <p className="text-sm leading-6 text-foreground-muted">{props.detail}</p>
      </section>
    </BridgeFrame>
  );
}

export function CommandPaletteOverlay(props: {
  open: boolean;
  inputRef: RefObject<HTMLInputElement | null>;
  query: string;
  onQueryChange: (value: string) => void;
  commands: ReadonlyArray<{
    action: string;
    category: string;
    label: string;
    shortcut: string;
  }>;
  selectedIndex: number;
  onHighlight: (index: number) => void;
  onSelect: (action: string) => void;
}) {
  if (!props.open) {
    return null;
  }

  // Group commands by category
  const grouped: Array<{ category: string; items: Array<{ index: number; action: string; label: string; shortcut: string }> }> = [];
  let currentCategory = "";
  for (let i = 0; i < props.commands.length; i++) {
    const cmd = props.commands[i];
    if (cmd.category !== currentCategory) {
      currentCategory = cmd.category;
      grouped.push({ category: cmd.category, items: [] });
    }
    grouped[grouped.length - 1].items.push({ index: i, action: cmd.action, label: cmd.label, shortcut: cmd.shortcut });
  }

  return (
    <div
      className="absolute inset-0 z-30 flex items-start justify-center px-4 pt-16"
      style={{
        background: "rgba(11, 15, 22, 0.75)",
        backdropFilter: "blur(8px)",
        animation: "fadeIn var(--duration-fast) var(--ease-out-expo)",
      }}
    >
      <section
        className="surface w-full max-w-2xl shadow-2xl"
        style={{
          borderRadius: "var(--radius-md)",
          animation: "scaleIn var(--duration-normal) var(--spring-command)",
          transformOrigin: "top center",
        }}
      >
        <div className="flex items-center gap-2.5 border-b border-outline px-4 py-3">
          <Command className="size-4 text-foreground-subtle" />
          <input
            ref={props.inputRef}
            className="flex-1 bg-transparent text-[length:var(--text-base)] text-foreground outline-none placeholder:text-foreground-subtle"
            placeholder="Search commands..."
            value={props.query}
            onChange={(event) => props.onQueryChange(event.target.value)}
          />
        </div>
        <div className="subtle-scrollbar max-h-[28rem] overflow-y-auto py-1">
          {grouped.map((group) => (
            <div key={group.category}>
              <div className="sticky top-0 z-10 bg-panel px-4 pb-1 pt-2">
                <span className="mono-meta">{group.category}</span>
              </div>
              {group.items.map((item) => (
                <button
                  key={`${group.category}-${item.action}`}
                  type="button"
                  aria-selected={props.selectedIndex === item.index}
                  className={cn(
                    "flex w-full items-center justify-between px-4 py-2 text-left transition-colors",
                    props.selectedIndex === item.index
                      ? "border-l-2 border-l-accent bg-panel-elevated text-foreground"
                      : "border-l-2 border-l-transparent text-foreground-muted hover:bg-panel-elevated/60 hover:text-foreground",
                  )}
                  onMouseEnter={() => props.onHighlight(item.index)}
                  onClick={() => props.onSelect(item.action)}
                >
                  <span className="truncate text-[length:var(--text-sm)]">{item.label}</span>
                  {item.shortcut ? (
                    <kbd
                      className="ml-3 inline-flex h-5 shrink-0 items-center border border-outline bg-canvas-elevated px-1.5 font-mono text-[length:var(--text-xs)] uppercase text-foreground-subtle"
                      style={{ borderRadius: "var(--radius-sm)" }}
                    >
                      {item.shortcut}
                    </kbd>
                  ) : null}
                </button>
              ))}
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

export function HelpOverlay(props: {
  open: boolean;
  sections: ReadonlyArray<{
    id: string;
    title: string;
    entries: ReadonlyArray<{ display: string; action: string; label: string }>;
  }>;
  onClose: () => void;
}) {
  if (!props.open) {
    return null;
  }

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-canvas/80 px-4 py-6">
      <section className="surface subtle-scrollbar w-full max-w-5xl overflow-y-auto px-4 py-4">
        <div className="flex items-center justify-between gap-4 border-b border-outline pb-3">
          <div>
            <p className="mono-meta">Help</p>
            <h2 className="mt-2 text-2xl font-semibold tracking-tight text-foreground">
              Keyboard map
            </h2>
          </div>
          <button
            type="button"
            className="border border-outline bg-panel-elevated px-2 py-1.5 text-xs uppercase text-foreground-muted"
            onClick={props.onClose}
          >
            Close
          </button>
        </div>
        <div className="mt-4 grid gap-4 xl:grid-cols-3">
          {props.sections.map((section) => (
            <section key={section.id} className="surface-muted px-3 py-3">
              <p className="mono-meta">{section.title}</p>
              <div className="mt-3 space-y-1">
                {section.entries.map((entry) => (
                  <div
                    key={`${section.id}-${entry.display}-${entry.action}`}
                    className="flex items-center justify-between gap-4 border-b border-outline/50 px-2 py-1.5 text-sm"
                  >
                    <span className="truncate text-foreground-muted">{entry.label}</span>
                    <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-foreground-subtle">
                      {entry.display}
                    </span>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      </section>
    </div>
  );
}

export function OnboardingOverlay(props: {
  open: boolean;
  onClose: () => void;
}) {
  if (!props.open) {
    return null;
  }

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-canvas/80 px-4 py-6">
      <section className="surface w-full max-w-3xl px-6 py-6">
        <div className="flex items-start justify-between gap-4 border-b border-outline pb-4">
          <div>
            <p className="mono-meta">Start Here</p>
            <h2 className="mt-2 text-2xl font-semibold tracking-tight text-foreground">
              Start here
            </h2>
            <p className="mt-3 max-w-2xl text-sm leading-6 text-foreground-muted">
              Desktop should make the main mail loop feel effortless: sync quietly, read locally,
              compose in your editor, and keep mailbox actions one shortcut away.
            </p>
          </div>
          <button
            type="button"
            className="rounded border border-outline bg-panel-elevated px-3 py-1.5 text-xs uppercase text-foreground-muted"
            onClick={props.onClose}
          >
            Close
          </button>
        </div>
        <div className="mt-5 grid gap-3 md:grid-cols-3">
          <article className="surface-muted px-4 py-4">
            <p className="mono-meta">1. Connect</p>
            <h3 className="mt-2 text-sm font-semibold text-foreground">Add an account</h3>
            <p className="mt-2 text-sm leading-6 text-foreground-muted">
              Gmail and IMAP feed the same local model so sync, search, and mutations stay
              consistent.
            </p>
          </article>
          <article className="surface-muted px-4 py-4">
            <p className="mono-meta">2. Read</p>
            <h3 className="mt-2 text-sm font-semibold text-foreground">Pick the right view</h3>
            <p className="mt-2 text-sm leading-6 text-foreground-muted">
              Reader mode stays plain-text first. HTML mode is there when fidelity matters, with
              remote content kept explicit.
            </p>
          </article>
          <article className="surface-muted px-4 py-4">
            <p className="mono-meta">3. Compose</p>
            <h3 className="mt-2 text-sm font-semibold text-foreground">Draft in $EDITOR</h3>
            <p className="mt-2 text-sm leading-6 text-foreground-muted">
              New mail, replies, and forwards all share the same draft session so save, reopen,
              and send stay predictable.
            </p>
          </article>
        </div>
      </section>
    </div>
  );
}

export function InboxZeroOverlay(props: { open: boolean; onDismiss: () => void }) {
  if (!props.open) {
    return null;
  }

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-[radial-gradient(circle_at_top,_rgba(103,183,255,0.20),_transparent_32%),linear-gradient(180deg,_rgba(21,28,44,0.96),_rgba(10,13,20,0.98))] px-6">
      <section className="mx-auto flex max-w-3xl flex-col items-center gap-4 text-center">
        <p className="mono-meta">Inbox zero</p>
        <h2 className="max-w-2xl text-5xl font-semibold tracking-tight text-foreground">
          Congratulations. You hit Inbox Zero.
        </h2>
        <p className="max-w-xl text-base leading-7 text-foreground-muted">
          Spend less time in your inbox, and more time on what matters most.
        </p>
        <HeaderActionButton label="Enter to dismiss" onClick={props.onDismiss} />
      </section>
    </div>
  );
}
