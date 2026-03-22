import { AlertTriangle, Command } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import type { BridgeState } from "../../shared/types";
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
      <section className="surface mx-auto flex w-full max-w-4xl flex-col gap-6 rounded-3xl px-8 py-8">
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="mono-meta">mxr Desktop</p>
            <h1 className="mt-3 text-4xl font-semibold tracking-tight text-foreground">
              mxr Desktop needs a compatible version of mxr
            </h1>
            <p className="mt-4 max-w-2xl text-sm leading-7 text-foreground-muted">
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
        <div className="flex flex-wrap gap-3">
          <button
            className="rounded-xl border border-accent/30 bg-accent/15 px-4 py-2 text-sm font-medium text-accent"
            onClick={props.onUseBundled}
          >
            Use bundled mxr
          </button>
          <button
            className="rounded-xl border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
            onClick={props.onRetry}
          >
            Retry
          </button>
        </div>
        <div className="surface-muted rounded-2xl px-5 py-5">
          <p className="mono-meta">Update steps</p>
          <ul className="mt-4 space-y-3 text-sm text-foreground-muted">
            {props.bridge.updateSteps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ul>
        </div>
        <div className="surface-muted grid gap-3 rounded-2xl px-5 py-5">
          <label className="mono-meta" htmlFor="external-binary">
            Advanced external mxr binary
          </label>
          <input
            id="external-binary"
            className="rounded-xl border border-outline bg-canvas-elevated px-4 py-3 text-sm text-foreground outline-none ring-0 placeholder:text-foreground-subtle"
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
            className="w-fit rounded-xl border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground-muted"
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
      <section className="surface mx-auto flex w-full max-w-3xl flex-col gap-6 rounded-3xl px-8 py-8">
        <p className="mono-meta">mxr Desktop</p>
        <h1 className="text-4xl font-semibold tracking-tight text-foreground">{props.title}</h1>
        <p className="max-w-2xl text-sm leading-7 text-foreground-muted">{props.detail}</p>
        <div className="surface-muted rounded-2xl px-5 py-5">
          <p className="mono-meta">Useful next steps</p>
          <ul className="mt-4 space-y-3 text-sm text-foreground-muted">
            {props.updateSteps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ul>
        </div>
        <button
          className="w-fit rounded-xl border border-outline bg-panel-elevated px-4 py-2 text-sm text-foreground"
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
      <section className="surface mx-auto flex w-full max-w-xl flex-col gap-4 rounded-3xl px-8 py-8">
        <p className="mono-meta">mxr Desktop</p>
        <h1 className="text-4xl font-semibold tracking-tight text-foreground">{props.title}</h1>
        <p className="text-sm leading-7 text-foreground-muted">{props.detail}</p>
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
  onSelect: (action: string) => void;
}) {
  if (!props.open) {
    return null;
  }

  return (
    <div className="absolute inset-0 z-30 flex items-start justify-center bg-canvas/60 px-6 pt-24 backdrop-blur-sm">
      <section className="surface w-full max-w-2xl rounded-3xl">
        <div className="flex items-center gap-3 border-b border-outline px-5 py-4">
          <Command className="size-4 text-foreground-subtle" />
          <input
            ref={props.inputRef}
            className="flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-foreground-subtle"
            placeholder="Search commands"
            value={props.query}
            onChange={(event) => props.onQueryChange(event.target.value)}
          />
        </div>
        <div className="max-h-[28rem] overflow-y-auto px-3 py-3">
          {props.commands.map((item) => (
            <button
              key={`${item.category}-${item.action}-${item.label}`}
              type="button"
              className="flex w-full items-center justify-between rounded-2xl px-3 py-3 text-left text-sm text-foreground-muted hover:bg-panel-elevated hover:text-foreground"
              onClick={() => props.onSelect(item.action)}
            >
              <div className="min-w-0">
                <div className="truncate text-sm text-foreground">{item.label}</div>
                <div className="mt-1 text-xs text-foreground-subtle">{item.category}</div>
              </div>
              <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-foreground-subtle">
                {item.shortcut || " "}
              </span>
            </button>
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
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-canvas/70 px-6 py-10">
      <section className="surface subtle-scrollbar w-full max-w-5xl overflow-y-auto rounded-3xl px-6 py-6">
        <div className="flex items-center justify-between gap-4 border-b border-outline pb-4">
          <div>
            <p className="mono-meta">Help</p>
            <h2 className="mt-3 text-3xl font-semibold tracking-tight text-foreground">
              Keyboard map
            </h2>
          </div>
          <button
            type="button"
            className="rounded-xl border border-outline bg-panel-elevated px-3 py-2 text-sm text-foreground-muted"
            onClick={props.onClose}
          >
            Close
          </button>
        </div>
        <div className="mt-6 grid gap-6 xl:grid-cols-3">
          {props.sections.map((section) => (
            <section key={section.id} className="surface-muted rounded-2xl px-4 py-4">
              <p className="mono-meta">{section.title}</p>
              <div className="mt-4 space-y-2">
                {section.entries.map((entry) => (
                  <div
                    key={`${section.id}-${entry.display}-${entry.action}`}
                    className="flex items-center justify-between gap-4 rounded-xl px-3 py-2 text-sm"
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

export function InboxZeroOverlay(props: { open: boolean; onDismiss: () => void }) {
  if (!props.open) {
    return null;
  }

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-[radial-gradient(circle_at_top,_rgba(103,183,255,0.20),_transparent_32%),linear-gradient(180deg,_rgba(21,28,44,0.96),_rgba(10,13,20,0.98))] px-6">
      <section className="mx-auto flex max-w-3xl flex-col items-center gap-6 text-center">
        <p className="mono-meta">Inbox zero</p>
        <h2 className="max-w-2xl text-6xl font-semibold tracking-tight text-foreground">
          Congratulations. You hit Inbox Zero.
        </h2>
        <p className="max-w-xl text-lg leading-8 text-foreground-muted">
          Spend less time in your inbox, and more time on what matters most.
        </p>
        <HeaderActionButton label="Enter to dismiss" onClick={props.onDismiss} />
      </section>
    </div>
  );
}
