import { AlertCircle, Diamond } from "lucide-react";
import { memo, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import type { UpdateLine } from "./api";
import {
  canLetGoSource,
  canPinSource,
  canSweepSource,
  canTune,
  openableLink,
  trackerStatus,
  type SuggestedSetting,
} from "./digestView";
import { useUpdatesHidden, useUpdatesPins } from "./updatesVerbs";

function Mark({ line }: { line: UpdateLine }) {
  if (line.section === "needs_a_look") {
    return <AlertCircle aria-hidden className="mt-0.5 size-3.5 shrink-0 text-warning" />;
  }
  if (line.section === "changed") {
    const delivered = line.tracker?.kind === "parcel" && line.tracker.state === "delivered";
    return (
      <Diamond
        aria-hidden
        className={cn(
          "mt-0.5 size-3 shrink-0",
          delivered ? "text-muted-foreground/50" : "text-primary",
        )}
      />
    );
  }
  return <span aria-hidden className="w-3.5 shrink-0" />;
}

/**
 * One source line: the source, its fact (written by rules, never a model),
 * a computed delta, a tracker's state, "already in To do", and its why
 * line. Opening the email is the last resort, so it is a quiet button.
 */
export const UpdateLineRow = memo(function UpdateLineRow({
  line,
  index,
  focused,
  onSelect,
  onLetGo,
  onNeedsMe,
  onTune,
  onOpenEmail,
  onTuneTo,
  onSweep,
  onPin,
  hint,
}: {
  line: UpdateLine;
  index: number;
  focused: boolean;
  onSelect: (line: UpdateLine) => void;
  onLetGo: (line: UpdateLine) => void;
  onNeedsMe: (line: UpdateLine) => void;
  onTune: (line: UpdateLine) => void;
  onOpenEmail: (line: UpdateLine) => void;
  onTuneTo: (line: UpdateLine, setting: SuggestedSetting) => void;
  onSweep: (line: UpdateLine) => void;
  onPin: (line: UpdateLine) => void;
  /** A hint anchored under this line's actions. */
  hint?: ReactNode;
}) {
  const leaving = useUpdatesHidden((s) => s.leaving.has(line.id));
  const hide = useUpdatesHidden((s) => s.hide);
  const pinned = useUpdatesPins((s) => s.pinned.has(line.id));
  const link = openableLink(line);
  const detail = line.tracker?.detail;
  const status = trackerStatus(line);
  const parcel = line.tracker?.kind === "parcel";
  const parcelItems = (line.tracker?.items ?? []).filter((item) => item.name.trim());
  const firstItem = parcelItems[0];
  const extraItems = Math.max(0, parcelItems.length - 1);
  const trackingNumber = line.tracker?.tracking_number?.trim() || undefined;
  const orderNumber = line.tracker?.order_number?.trim() || undefined;
  const reference = trackingNumber ?? orderNumber;
  const detailContext = reference
    ? ` with reference ${reference}`
    : firstItem
      ? ` for item ${firstItem.name}`
      : "";
  return (
    <li
      data-index={index}
      data-testid="update-line"
      data-line-id={line.id}
      data-section={line.section}
      data-source={line.source_key}
      data-focused={focused ? "true" : undefined}
      onMouseEnter={() => onSelect(line)}
      onFocusCapture={() => onSelect(line)}
      onAnimationEnd={leaving ? () => hide([line.id]) : undefined}
      className={cn(
        "relative mx-2 rounded-md px-3 py-2",
        focused ? "bg-accent" : "hover:bg-accent/40",
        leaving && "todo-fold pointer-events-none",
      )}
    >
      {focused ? (
        <span aria-hidden className="absolute inset-y-1.5 left-0 w-[2px] rounded-full bg-primary" />
      ) : null}
      <div className="flex min-w-0 items-start gap-2">
        <Mark line={line} />
        <div className="min-w-0 flex-1">
          <p className="flex min-w-0 flex-col gap-x-3 text-[13px] @xl:flex-row @xl:items-baseline">
            <span
              data-testid="update-source"
              className="shrink-0 font-medium text-foreground/90 @xl:w-44 @xl:truncate"
            >
              {line.source_name}
            </span>
            <span className="min-w-0 break-words text-foreground/90">
              <span
                data-testid="update-fact"
                className={cn(
                  parcel && line.tracker?.state === "delivered" && "text-muted-foreground",
                  parcel && line.tracker?.state !== "delivered" && "font-medium",
                )}
              >
                {line.fact}
              </span>
              {line.delta ? (
                <span
                  data-testid="update-delta"
                  title={`Computed from ${line.delta.raw} and ${line.delta.previous_raw}`}
                  className="ml-2 whitespace-nowrap text-[12px] text-primary"
                >
                  {line.delta.text}
                </span>
              ) : null}
              {line.time_label ? (
                <span className="ml-2 text-[12px] tabular-nums text-muted-foreground">
                  {line.time_label}
                </span>
              ) : null}
            </span>
            {line.count > 1 && line.section === "routine" ? (
              <span className="ml-auto shrink-0 text-[12px] tabular-nums text-muted-foreground">
                {line.count}
              </span>
            ) : null}
          </p>
          {parcel && (firstItem || reference) ? (
            <p data-testid="parcel-identity" className="mt-0.5 break-words text-[12px] text-fg-2">
              {firstItem ? (
                <span>
                  {firstItem.name}
                  {extraItems > 0 ? ` +${extraItems} more` : null}
                </span>
              ) : null}
              {reference ? (
                <span>
                  {firstItem ? " · " : null}
                  {trackingNumber ? "Tracking " : "Order "}{reference}
                </span>
              ) : null}
            </p>
          ) : null}
          {detail || status ? (
            <p className="mt-0.5 text-[12px] text-fg-2">
              {status}
              {status && detail ? " · " : null}
              {detail}
            </p>
          ) : null}
          {!parcel ? (
            <p data-testid="update-why" className="mt-0.5 text-[12px] text-fg-2">
              {line.why}
            </p>
          ) : null}
          {line.todo_suggestion ? (
            <p
              data-testid="update-todo-suggestion"
              className="mt-1 flex flex-wrap items-center gap-2 rounded border border-warning/40 bg-warning/10 px-2 py-1 text-[12.5px] font-medium text-foreground"
            >
              {line.todo_suggestion}
              <Button size="sm" variant="outline" onClick={() => onNeedsMe(line)}>
                Add to To do
              </Button>
            </p>
          ) : null}
          {line.suggestion ? (
            <p
              data-testid="update-suggestion"
              className="mt-1 flex flex-wrap items-center gap-2 text-[12.5px] text-foreground/90"
            >
              {line.suggestion}
              <Button size="sm" variant="outline" onClick={() => onTuneTo(line, "muted")}>
                Mute
              </Button>
              <Button size="sm" variant="ghost" onClick={() => onTuneTo(line, "changes_only")}>
                Changes only
              </Button>
            </p>
          ) : null}
          <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
            {parcel ? (
              <details className="text-[12px] text-fg-2 open:basis-full">
                <summary
                  aria-label={`Details for ${line.source_name} parcel${detailContext}`}
                  className="w-fit cursor-pointer rounded-sm underline decoration-border-strong underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  Details
                </summary>
                <div className="mt-1 w-full space-y-1 break-words pl-1">
                  {parcelItems.length > 0 ? (
                    <ul aria-label="Available parcel items" className="list-inside list-disc">
                      {parcelItems.map((item) => (
                        <li key={`${item.name}:${item.quantity ?? ""}`}>
                          {item.name}{item.quantity !== undefined ? ` · quantity ${item.quantity}` : ""}
                        </li>
                      ))}
                    </ul>
                  ) : null}
                  {trackingNumber ? <p>Tracking reference: {trackingNumber}</p> : null}
                  {orderNumber ? <p>Order reference: {orderNumber}</p> : null}
                  <p data-testid="update-why">{line.why}</p>
                </div>
              </details>
            ) : null}
            {line.in_todo ? (
              <span
                data-testid="update-in-todo"
                className="rounded border border-border px-1.5 py-0.5 text-[11.5px] text-muted-foreground"
              >
                {line.in_todo}
              </span>
            ) : line.latest_message_id ? (
              <Button size="sm" variant="outline" onClick={() => onNeedsMe(line)}>
                This needs me
              </Button>
            ) : null}
            {canPinSource(line) ? (
              <Button size="sm" variant="ghost" aria-pressed={pinned} onClick={() => onPin(line)}>
                {pinned ? "Unpin" : "Pin"}
              </Button>
            ) : null}
            {canSweepSource(line) ? (
              <Button size="sm" variant="ghost" onClick={() => onSweep(line)}>
                Sweep sender
              </Button>
            ) : null}
            {canLetGoSource(line) ? (
              <Button size="sm" variant="ghost" onClick={() => onLetGo(line)}>
                Let go
              </Button>
            ) : null}
            {canTune(line) ? (
              <Button size="sm" variant="ghost" onClick={() => onTune(line)}>
                Tune
              </Button>
            ) : null}
            {link ? (
              <Button asChild size="sm" variant="ghost">
                <a
                  href={link.url}
                  target="_blank"
                  rel="noreferrer noopener"
                  title={`Opens ${link.domain}`}
                >
                  Open {link.domain}
                </a>
              </Button>
            ) : null}
            {line.latest_thread_id ? (
              <Button size="sm" variant="ghost" onClick={() => onOpenEmail(line)}>
                {parcel ? "From email" : "Email"}
              </Button>
            ) : null}
          </div>
          {hint}
        </div>
      </div>
    </li>
  );
});
