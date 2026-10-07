import { AlertCircle, Diamond } from "lucide-react";
import { memo } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import type { UpdateLine } from "./api";
import { canLetGoSource, canTune, openableLink, trackStep } from "./digestView";
import { useUpdatesHidden } from "./updatesVerbs";

/** A parcel's steps as dots on a line, the current one filled. */
function Track({ line }: { line: UpdateLine }) {
  const { steps, step } = trackStep(line);
  if (steps.length === 0) return null;
  return (
    <ol
      aria-label={`Parcel: ${line.tracker?.state_label ?? ""}`}
      data-testid="parcel-track"
      className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5 text-[11.5px] text-muted-foreground"
    >
      {steps.map((label, at) => {
        const reached = step !== null && at <= step;
        const current = step === at;
        return (
          <li key={label} className="inline-flex items-center gap-1">
            <span
              aria-hidden
              className={cn(
                "inline-block size-1.5 rounded-full",
                reached ? "bg-primary" : "border border-border-strong",
              )}
            />
            <span className={cn(current && "font-medium text-foreground")}>{label}</span>
          </li>
        );
      })}
    </ol>
  );
}

function Mark({ line }: { line: UpdateLine }) {
  if (line.section === "needs_a_look") {
    return <AlertCircle aria-hidden className="mt-0.5 size-3.5 shrink-0 text-warning" />;
  }
  if (line.section === "changed") {
    return <Diamond aria-hidden className="mt-0.5 size-3 shrink-0 text-primary" />;
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
}: {
  line: UpdateLine;
  index: number;
  focused: boolean;
  onSelect: (line: UpdateLine) => void;
  onLetGo: (line: UpdateLine) => void;
  onNeedsMe: (line: UpdateLine) => void;
  onTune: (line: UpdateLine) => void;
  onOpenEmail: (line: UpdateLine) => void;
  onTuneTo: (line: UpdateLine, setting: "muted" | "changes_only") => void;
}) {
  const leaving = useUpdatesHidden((s) => s.leaving.has(line.id));
  const hide = useUpdatesHidden((s) => s.hide);
  const link = openableLink(line);
  const detail = line.tracker?.detail;
  return (
    <li
      data-index={index}
      data-testid="update-line"
      data-section={line.section}
      data-source={line.source_key}
      data-focused={focused ? "true" : undefined}
      onMouseEnter={() => onSelect(line)}
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
              <span data-testid="update-fact">{line.fact}</span>
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
          {line.tracker?.kind === "parcel" ? <Track line={line} /> : null}
          {detail || (line.tracker && line.tracker.kind !== "parcel") ? (
            <p className="mt-0.5 text-[12px] text-muted-foreground">
              {line.tracker && line.tracker.kind !== "parcel"
                ? `${line.tracker.kind === "build" ? "Build" : "Incident"} ${line.tracker.state_label}`
                : null}
              {line.tracker && line.tracker.kind !== "parcel" && detail ? " · " : null}
              {detail}
            </p>
          ) : null}
          <p data-testid="update-why" className="mt-0.5 text-[12px] text-muted-foreground">
            {line.why}
          </p>
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
          <p className="mt-1.5 flex flex-wrap items-center gap-1.5">
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
                Email
              </Button>
            ) : null}
          </p>
        </div>
      </div>
    </li>
  );
});
