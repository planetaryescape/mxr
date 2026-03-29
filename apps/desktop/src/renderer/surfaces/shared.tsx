import { cn } from "../lib/cn";

export function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div
      className="border border-outline bg-canvas-elevated px-3 py-3"
      style={{ borderRadius: "var(--radius-sm)" }}
    >
      <p className="mono-meta">{label}</p>
      <p className="mt-1.5 text-[length:var(--text-lg)] font-semibold text-foreground">{value}</p>
    </div>
  );
}

export function HeaderActionButton(props: {
  label: string;
  shortcut?: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={props.label}
      disabled={props.disabled}
      className={cn(
        "flex h-6 items-center gap-1.5 border px-2 text-[length:var(--text-xs)] transition-colors",
        props.disabled
          ? "border-outline/60 bg-canvas-elevated text-foreground-subtle"
          : "border-outline bg-canvas-elevated text-foreground-muted hover:border-outline-strong hover:bg-panel-elevated hover:text-foreground",
      )}
      style={{ borderRadius: "var(--radius-sm)" }}
      onClick={props.onClick}
    >
      <span>{props.label}</span>
      {props.shortcut ? (
        <kbd aria-hidden="true" className="font-mono text-[length:var(--text-2xs)] uppercase text-foreground-subtle">
          {props.shortcut}
        </kbd>
      ) : null}
    </button>
  );
}
