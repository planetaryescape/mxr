import { MoreHorizontal } from "lucide-react";
import { memo, useRef, useState, type ReactNode } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";

import type { UpdateLine } from "./api";
import {
  canLetGoSource,
  canPinSource,
  canSweepSource,
  canTune,
  openableLink,
  splitSourceName,
  trackerStatus,
  trackLabel,
  trackReplacesFact,
  trailingItem,
  type SuggestedSetting,
  type TrailingItem,
} from "./digestView";
import { useUpdatesHidden } from "./updatesVerbs";

/**
 * Clicks on these belong to the control or the peek inside the row, not to
 * the row body, so they never toggle the peek. Menus render in a portal but
 * their events still bubble to the row in React, hence the menu selector.
 */
const NOT_ROW_BODY = "button, a, [role='menu'], [role='region'], [role='status']";

/**
 * One source line in the reel: the source, its fact, one trailing value, and
 * a more menu. The rest (why, numbers, references, the suggestion, the links)
 * sits in the peek that Space opens under the row.
 */
export const UpdateLineRow = memo(function UpdateLineRow({
  line,
  index,
  focused,
  peeked,
  onSelect,
  onLetGo,
  onNeedsMe,
  onTune,
  onOpenEmail,
  onTuneTo,
  onSweep,
  onPin,
  onTogglePeek,
  hint,
}: {
  line: UpdateLine;
  index: number;
  focused: boolean;
  peeked: boolean;
  onSelect: (line: UpdateLine) => void;
  onLetGo: (line: UpdateLine) => void;
  onNeedsMe: (line: UpdateLine) => void;
  onTune: (line: UpdateLine) => void;
  onOpenEmail: (line: UpdateLine) => void;
  onTuneTo: (line: UpdateLine, setting: SuggestedSetting) => void;
  onSweep: (line: UpdateLine) => void;
  onPin: (line: UpdateLine) => void;
  onTogglePeek: (line: UpdateLine) => void;
  /** A hint anchored under this line's actions. */
  hint?: ReactNode;
}) {
  const leaving = useUpdatesHidden((s) => s.leaving.has(line.id));
  const hide = useUpdatesHidden((s) => s.hide);
  const [menuOpen, setMenuOpen] = useState(false);
  const rowRef = useRef<HTMLLIElement>(null);
  // Actions that open a dialog keep focus with the dialog when the menu closes.
  const returnFocus = useRef(true);
  const toDialog = (action: () => void) => () => {
    returnFocus.current = false;
    action();
  };
  const pinned = line.latest_message_pinned ?? false;
  const link = openableLink(line);
  const { name, host } = splitSourceName(line);
  const parcel = line.tracker?.kind === "parcel";
  const delivered = parcel && line.tracker?.state === "delivered";
  const trailing = trailingItem(line);
  const track = parcel ? trackLabel(line) : null;
  const trackStands = trackReplacesFact(line, track);
  const expanded = focused || peeked;
  const canToDo = !line.in_todo && Boolean(line.todo_suggestion || line.latest_message_id);
  // The keys worth naming on hover or focus, after the verbs the row already shows.
  const keyHints = [
    canToDo && !line.todo_suggestion ? { key: "t", verb: "To do" } : null,
    canLetGoSource(line) ? { key: "e", verb: "Let go" } : null,
    line.latest_thread_id ? { key: "o", verb: "Email" } : null,
  ]
    .filter((keyHint) => keyHint !== null)
    .slice(0, 2);
  // A value gives way to the key hints while the row is focused or hovered;
  // the to-do affordance is a control and stays put.

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
      ref={rowRef}
      // Focus comes back here when the menu closes, so Space peeks in the normal flow.
      tabIndex={-1}
      onClick={(event) => {
        const target = event.target;
        if (!(target instanceof Element) || target.closest(NOT_ROW_BODY)) return;
        onTogglePeek(line);
      }}
      onAnimationEnd={leaving ? () => hide([line.id]) : undefined}
      className={cn(
        "group/updates relative mx-2 rounded-md px-3 py-2 focus:outline-none",
        focused ? "bg-accent" : "hover:bg-accent/40",
        leaving && "todo-fold pointer-events-none",
      )}
    >
      {focused ? (
        <span aria-hidden className="absolute inset-y-1.5 left-0 w-[2px] rounded-full bg-primary" />
      ) : null}
      <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-0.5 text-sm @xl:flex-nowrap">
        <span
          data-testid="update-source"
          title={line.source_name}
          className={cn(
            "order-1 flex min-w-0 flex-1 items-baseline gap-1 whitespace-nowrap font-normal @xl:w-52 @xl:flex-none @xl:shrink-0",
            line.section === "needs_a_look" ? "text-foreground" : "text-fg-2",
          )}
        >
          <span className="shrink-0">{name}</span>
          {host ? (
            <span
              data-testid="update-host"
              className="min-w-0 truncate text-2xs text-muted-foreground"
            >
              {` (${host})`}
            </span>
          ) : null}
        </span>
        <span
          data-testid="update-fact"
          title={line.fact}
          className={cn(
            "order-4 min-w-0 basis-full text-foreground/90 @xl:order-2 @xl:basis-auto @xl:flex-1",
            expanded ? "break-words" : "truncate",
            delivered && "text-muted-foreground",
            parcel && !delivered && "font-medium",
          )}
        >
          {trackStands ? track : line.fact}
          {track && !trackStands ? (
            <span className="ml-2 text-xs text-muted-foreground">{track}</span>
          ) : null}
        </span>
        <span className="order-2 ml-auto flex shrink-0 items-center gap-2 text-xs tabular-nums @xl:order-3">
          {trailing ? (
            <TrailingValue
              item={trailing}
              focused={focused}
              todoTitle={line.todo_title}
              onNeedsMe={() => onNeedsMe(line)}
            />
          ) : null}
          <span
            aria-hidden
            data-testid="update-keys"
            className={cn(
              "hidden items-center gap-2 text-2xs text-muted-foreground",
              focused ? "@xl:flex" : "@xl:pointer-fine:group-hover/updates:flex",
            )}
          >
            {[...keyHints, { key: "Space", verb: "Details" }].map((keyHint) => (
              <span key={keyHint.key} className="inline-flex items-center gap-1">
                <KeyChip className="h-4 px-1">{keyHint.key}</KeyChip>
                {keyHint.verb}
              </span>
            ))}
          </span>
        </span>
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={`More actions for ${name}`}
              data-testid="update-more"
              className={cn(
                "order-3 shrink-0 @xl:order-4",
                focused || menuOpen
                  ? "opacity-100"
                  : "@xl:opacity-0 @xl:group-hover/updates:opacity-100 @xl:group-focus-within/updates:opacity-100",
              )}
            >
              <MoreHorizontal aria-hidden />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="end"
            className="min-w-44"
            onCloseAutoFocus={(event) => {
              // Radix would focus the More trigger; the row is where the keys are.
              event.preventDefault();
              if (returnFocus.current) rowRef.current?.focus();
              returnFocus.current = true;
            }}
          >
            {canToDo ? (
              <DropdownMenuItem onSelect={() => onNeedsMe(line)}>
                {line.todo_suggestion ? "Add to To do" : "This needs me"}
              </DropdownMenuItem>
            ) : null}
            {link ? (
              <DropdownMenuItem asChild>
                <a href={link.url} target="_blank" rel="noreferrer noopener">
                  Open {link.domain}
                </a>
              </DropdownMenuItem>
            ) : null}
            {line.latest_thread_id ? (
              <DropdownMenuItem onSelect={toDialog(() => onOpenEmail(line))}>
                Open email
              </DropdownMenuItem>
            ) : null}
            {canPinSource(line) ? (
              <DropdownMenuItem onSelect={() => onPin(line)}>
                {pinned ? "Unpin" : "Pin"}
              </DropdownMenuItem>
            ) : null}
            {canSweepSource(line) ? (
              <DropdownMenuItem onSelect={toDialog(() => onSweep(line))}>
                Sweep sender
              </DropdownMenuItem>
            ) : null}
            {canLetGoSource(line) ? (
              <DropdownMenuItem onSelect={toDialog(() => onLetGo(line))}>Let go</DropdownMenuItem>
            ) : null}
            {canTune(line) ? (
              <DropdownMenuItem onSelect={toDialog(() => onTune(line))}>Tune…</DropdownMenuItem>
            ) : null}
            {line.suggestion ? (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={() => onTuneTo(line, "muted")}>Mute</DropdownMenuItem>
                <DropdownMenuItem onSelect={() => onTuneTo(line, "changes_only")}>
                  Changes only
                </DropdownMenuItem>
              </>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {peeked ? (
        <Peek line={line} trackStands={trackStands} onOpenEmail={onOpenEmail} onTuneTo={onTuneTo} />
      ) : null}
      {hint}
    </li>
  );
});

/**
 * The one trailing value. The to-do affordance is a control, the rest are
 * text; text gives way to the key hints while the row is focused or hovered.
 */
function TrailingValue({
  item,
  focused,
  todoTitle,
  onNeedsMe,
}: {
  item: TrailingItem;
  focused: boolean;
  todoTitle: string;
  onNeedsMe: () => void;
}) {
  const hide = cn(focused && "@xl:hidden", "@xl:pointer-fine:group-hover/updates:hidden");
  switch (item.kind) {
    case "in_todo":
      return (
        <span
          data-testid="update-in-todo"
          className={cn(
            "whitespace-nowrap rounded border border-border px-1.5 py-0.5 text-2xs text-muted-foreground",
            hide,
          )}
        >
          {item.text}
        </span>
      );
    case "todo_suggestion":
      return (
        <Button
          data-testid="update-todo-suggestion"
          size="xs"
          variant="outline"
          aria-label={`Add to To do: ${todoTitle}`}
          onClick={onNeedsMe}
          className="border-warning/50 text-warning hover:bg-warning/10"
        >
          To do <KeyChip className="h-4 px-1">t</KeyChip>
        </Button>
      );
    case "delta":
      return (
        <span
          data-testid="update-delta"
          title={item.title}
          className={cn("whitespace-nowrap text-muted-foreground", hide)}
        >
          {item.text}
        </span>
      );
    case "amount":
      return (
        <span
          data-testid="update-amount"
          className={cn("whitespace-nowrap text-muted-foreground", hide)}
        >
          {item.text}
        </span>
      );
    case "count":
      return (
        <span
          data-testid="update-count"
          className={cn("whitespace-nowrap text-muted-foreground", hide)}
        >
          {item.text}
        </span>
      );
  }
}

/**
 * What the reel leaves out, opened with Space: why, the numbers as the mail
 * quoted them, a tracker's state, a parcel's items and references, the
 * suggestion with its two answers, and the link and the email.
 */
function Peek({
  line,
  trackStands,
  onOpenEmail,
  onTuneTo,
}: {
  line: UpdateLine;
  /** The track is in the fact column, so the peek holds the fact it replaced. */
  trackStands: boolean;
  onOpenEmail: (line: UpdateLine) => void;
  onTuneTo: (line: UpdateLine, setting: SuggestedSetting) => void;
}) {
  const { name } = splitSourceName(line);
  const link = openableLink(line);
  const tracker = line.tracker;
  const parcelItems = (tracker?.items ?? []).filter((item) => item.name.trim());
  const trackingNumber = tracker?.tracking_number?.trim() || undefined;
  const orderNumber = tracker?.order_number?.trim() || undefined;
  const status = [trackerStatus(line), tracker?.detail].filter(Boolean).join(" · ");
  const numbers = line.numbers ?? [];
  return (
    <div
      role="region"
      aria-label={`Details for ${name}`}
      data-testid="update-peek"
      className="mt-2 min-w-0 space-y-1.5 border-l-2 border-border pl-3 text-xs break-words text-fg-2"
    >
      {trackStands ? <p data-testid="update-fact-full">{line.fact}</p> : null}
      <p data-testid="update-why">{line.why}</p>
      {numbers.length > 0 ? (
        <p data-testid="update-numbers">
          Numbers: {numbers.map((number) => number.raw).join(", ")}
        </p>
      ) : null}
      {status ? <p>{status}</p> : null}
      {parcelItems.length > 0 ? (
        <ul aria-label="Available parcel items" className="list-inside list-disc">
          {parcelItems.map((item, itemIndex) => (
            // DTOs allow duplicate records and provide no stable item ID; this list is static.
            // oxlint-disable-next-line react/no-array-index-key
            <li key={`${itemIndex}:${item.name}:${item.quantity ?? ""}`}>
              {item.name}
              {item.quantity !== undefined ? ` · quantity ${item.quantity}` : ""}
            </li>
          ))}
        </ul>
      ) : null}
      {trackingNumber || orderNumber ? (
        <p data-testid="parcel-identity">
          {trackingNumber ? `Tracking reference: ${trackingNumber}` : null}
          {trackingNumber && orderNumber ? " · " : null}
          {orderNumber ? `Order reference: ${orderNumber}` : null}
        </p>
      ) : null}
      {line.time_label ? <p>At {line.time_label}</p> : null}
      {line.suggestion ? (
        <div data-testid="update-suggestion" className="space-y-1.5">
          <p className="text-foreground/90">{line.suggestion}</p>
          <div className="flex flex-wrap gap-1.5">
            <Button size="sm" variant="outline" onClick={() => onTuneTo(line, "muted")}>
              Mute
            </Button>
            <Button size="sm" variant="ghost" onClick={() => onTuneTo(line, "changes_only")}>
              Changes only
            </Button>
          </div>
        </div>
      ) : null}
      {link || line.latest_thread_id ? (
        <div className="flex flex-wrap gap-1.5 pt-0.5">
          {link ? (
            <Button asChild size="sm" variant="outline">
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
            <Button size="sm" variant="outline" onClick={() => onOpenEmail(line)}>
              Open email
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
