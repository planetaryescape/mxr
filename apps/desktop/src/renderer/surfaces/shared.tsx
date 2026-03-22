import { cn } from "../lib/cn";

export function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl border border-outline bg-canvas-elevated px-3 py-3">
      <p className="mono-meta">{label}</p>
      <p className="mt-2 text-lg font-semibold text-foreground">{value}</p>
    </div>
  );
}

export function HeaderActionButton(props: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={props.disabled}
      className={cn(
        "h-7 rounded-md border px-2.5 text-[11px] transition-colors",
        props.disabled
          ? "border-outline/60 bg-canvas-elevated text-foreground-subtle"
          : "border-outline bg-canvas-elevated text-foreground-muted hover:border-outline-strong hover:bg-panel-elevated hover:text-foreground",
      )}
      onClick={props.onClick}
    >
      {props.label}
    </button>
  );
}
