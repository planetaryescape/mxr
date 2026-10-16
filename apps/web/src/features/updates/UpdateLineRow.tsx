import { useQuery } from "@tanstack/react-query";
import { MoreHorizontal } from "lucide-react";
import { memo, useMemo, useRef, useState, type ReactNode } from "react";

import { ModelMark } from "@/components/ModelMark";
import { Row, RowVerb, type LampVia } from "@/components/Row";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useRowGist, type RowGist } from "@/features/gists/rowGists";
import { fetchThread } from "@/features/mailbox/api";
import { initials, plural } from "@/lib/format";
import { cn } from "@/lib/utils";

import type { UpdateLine } from "./api";
import {
  canLetGoSource,
  canPinSource,
  canSweepSource,
  canTune,
  gistThread,
  openableLink,
  displaySourceName,
  splitSourceName,
  trackerStatus,
  trackLabel,
  trackReplacesFact,
  trailingItem,
  type SuggestedSetting,
  type TrailingItem,
} from "./digestView";
import { previewLines, previewSource } from "./emailPreview";
import { useUpdatesHidden } from "./updatesVerbs";

/**
 * Clicks on these belong to the control or the peek inside the row, not to
 * the row body, so they never toggle the peek. Menus render in a portal but
 * their events still bubble to the row in React, hence the menu selector.
 */
/** A fact this long may not fit on the row's one line. */
const LONG_FACT = 70;

/** A phone taps the row to peek, and its width belongs to the source's name: these verbs wait for sm. */
const PHONE_HIDDEN = "hidden sm:inline-flex";

const NOT_ROW_BODY = "button, a, [role='menu'], [role='region'], [role='status']";

/**
 * One source line in the reel, on the shared `Row`: the source and its fact
 * on one line, one trailing value, and the verbs when lit. The rest (why,
 * numbers, references, the suggestion, the links) sits in the peek that Space
 * opens under the row.
 */
export const UpdateLineRow = memo(function UpdateLineRow({
  line,
  index,
  lit,
  via,
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
  lit: boolean;
  via: LampVia;
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
  const { name: rawName, host } = splitSourceName(line);
  const name = displaySourceName(rawName);
  const parcel = line.tracker?.kind === "parcel";
  const delivered = parcel && line.tracker?.state === "delivered";
  const track = parcel ? trackLabel(line) : null;
  const trackStands = trackReplacesFact(line, track);
  // The model's one-line gist stands in for a subject-only fact once it has
  // arrived, and only for the email the line is about. Until then (or with no
  // model) the subject shows exactly as it did.
  const gistThreadId = gistThread(line);
  const stored = useRowGist(gistThreadId ?? "");
  const gist =
    stored && stored.newest !== null && stored.newest === line.latest_message_id
      ? stored
      : undefined;
  const trailing = trailingItem(line, gist?.about ?? line.fact);
  const canToDo = !line.in_todo && Boolean(line.todo_suggestion || line.latest_message_id);
  const fact = trackStands ? track : (gist?.about ?? line.fact);
  const landIds = useMemo(
    () => [...(line.thread_ids ?? []), ...(line.message_ids ?? []).slice(-20)],
    [line.thread_ids, line.message_ids],
  );

  return (
    <Row
      ref={rowRef}
      data-index={index}
      data-testid="update-line"
      data-line-id={line.id}
      data-section={line.section}
      data-source={line.source_key}
      data-focused={lit ? "true" : undefined}
      lit={lit}
      via={via}
      leaving={leaving}
      land={landIds}
      // Focus comes back here when the menu closes, so Space peeks in the normal flow.
      tabIndex={-1}
      onClick={(event) => {
        const target = event.target;
        if (target instanceof Element && target.closest(NOT_ROW_BODY)) return;
        onSelect(line);
        onTogglePeek(line);
      }}
      onFocusCapture={() => onSelect(line)}
      onAnimationEnd={leaving ? () => hide([line.id]) : undefined}
      lead={<SourceMark name={name} />}
      primary={
        <>
          <span
            data-testid="update-source"
            title={line.source_name}
            className={cn("font-medium", line.section !== "needs_a_look" && "font-normal")}
          >
            {name}
            {host ? (
              <span data-testid="update-host" className="type-meta ml-1 text-text-3 max-sm:hidden">
                {` (${host})`}
              </span>
            ) : null}
          </span>
          <span
            data-testid="update-fact"
            data-gist={gist ? "true" : undefined}
            title={line.fact}
            className={cn("ml-3 text-text-2", delivered && "text-text-3")}
          >
            {fact}
            {track && !trackStands ? (
              <span className="type-meta ml-2 text-text-3">{track}</span>
            ) : null}
          </span>
        </>
      }
      trail={
        trailing ? (
          <TrailingValue
            item={trailing}
            todoTitle={line.todo_title}
            onNeedsMe={() => onNeedsMe(line)}
          />
        ) : null
      }
      verbs={
        <>
          <RowVerb
            keys="Space"
            label="peek"
            className={PHONE_HIDDEN}
            onClick={() => onTogglePeek(line)}
          />
          {canLetGoSource(line) ? (
            <RowVerb keys="e" label="let go" onClick={() => onLetGo(line)} />
          ) : null}
          {canToDo ? (
            <RowVerb
              keys="t"
              label="to-do"
              className={PHONE_HIDDEN}
              ariaLabel={
                line.todo_suggestion ? `Add to To do: ${line.todo_title}` : "This needs me: to-do"
              }
              onClick={() => onNeedsMe(line)}
            />
          ) : null}
          {line.latest_thread_id ? (
            <RowVerb
              keys="o"
              label="email"
              className={PHONE_HIDDEN}
              onClick={() => onOpenEmail(line)}
            />
          ) : null}
          <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={`More actions for ${name}`}
                data-testid="update-more"
                onClick={(event) => event.stopPropagation()}
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
        </>
      }
      hint={
        <>
          {peeked ? (
            <Peek
              line={line}
              gist={gist}
              showFact={trackStands || (!gist && line.fact.length > LONG_FACT)}
              onOpenEmail={onOpenEmail}
              onTuneTo={onTuneTo}
            />
          ) : null}
          {hint}
        </>
      }
    />
  );
});

/** The source's initials on a square tile: a mark for the source, not a person. */
function SourceMark({ name }: { name: string }) {
  return (
    <span
      aria-hidden
      className="type-meta grid size-7 shrink-0 place-items-center bg-ink-lamp text-text-2"
    >
      {initials(name)}
    </span>
  );
}

/** The one trailing value; the to-do suggestion is a control, the rest are text. */
function TrailingValue({
  item,
  todoTitle,
  onNeedsMe,
}: {
  item: TrailingItem;
  todoTitle: string;
  onNeedsMe: () => void;
}) {
  switch (item.kind) {
    case "in_todo":
      return <span data-testid="update-in-todo">{item.text}</span>;
    case "todo_suggestion":
      return (
        <Button
          data-testid="update-todo-suggestion"
          size="xs"
          variant="ghost"
          aria-label={`Add to To do: ${todoTitle}`}
          title="Add to To do (t)"
          onClick={(event) => {
            event.stopPropagation();
            onNeedsMe();
          }}
          className="text-text-2 hover:text-text-1"
        >
          Add to To do?
        </Button>
      );
    case "delta":
      return (
        <span data-testid="update-delta" title={item.title} className="whitespace-nowrap">
          {item.text}
        </span>
      );
    case "amount":
      return (
        <span data-testid="update-amount" className="whitespace-nowrap">
          {item.text}
        </span>
      );
    case "count":
      return (
        <span data-testid="update-count" className="whitespace-nowrap">
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
  gist,
  showFact,
  onOpenEmail,
  onTuneTo,
}: {
  line: UpdateLine;
  /** The gist the row shows in place of the subject, if it does. */
  gist: RowGist | undefined;
  /** The fact doesn't fit the row's one line, or the track stands in for it. */
  showFact: boolean;
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
      className="type-body mx-5 mb-2 ml-16 mt-1 min-w-0 space-y-2 break-words border-l border-rule pl-3 text-text-2"
    >
      {/* The row truncates the fact to one line, so a long one is whole here. */}
      {showFact ? <p data-testid="update-fact-full">{line.fact}</p> : null}
      {gist ? (
        <>
          <p data-testid="update-subject">Subject: {line.fact}</p>
          {gist.ask ? (
            <p data-testid="update-gist-ask">
              <span className="font-medium text-text-1">Asks:</span> {gist.ask}
            </p>
          ) : null}
        </>
      ) : null}
      <EmailPreview line={line} />
      <p data-testid="update-why">{line.why}</p>
      {numbers.length > 0 ? (
        <p data-testid="update-numbers">
          Numbers: {numbers.map((number) => number.raw).join(", ")}
        </p>
      ) : null}
      {status ? <p>{status}</p> : null}
      {/* Repeats of one alert fold into the line; the row shows the latest, the peek says how many. */}
      {!tracker && line.count > 1 ? (
        <p data-testid="update-count-folded">{plural(line.count, "email")} in this digest</p>
      ) : null}
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
      {gist ? (
        <p data-testid="update-gist-source">
          <EmailSource gist={gist} />
        </p>
      ) : null}
      {line.suggestion ? (
        <div data-testid="update-suggestion" className="space-y-1.5">
          <p className="text-text-1">{line.suggestion}</p>
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
            <Button
              size="sm"
              variant="ghost"
              title="Open the email (o)"
              onClick={() => onOpenEmail(line)}
            >
              Open email
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** "gemma4 · local · from this email": where the line's words came from. */
function EmailSource({ gist }: { gist: RowGist }) {
  const provenance = gist.provenance;
  if (!provenance) return <span className="type-meta text-text-3">{gist.source}</span>;
  const cloud = provenance.locality === "cloud";
  return (
    <>
      <ModelMark model={provenance.model} leavesMachine={cloud} />
      <span className="type-meta text-text-3">
        {" "}
        · {cloud ? "from this email only" : "from this email"}
      </span>
    </>
  );
}

/**
 * The first lines of the line's latest email. The thread is fetched only
 * once the peek is open, through the request the reader uses, so opening the
 * email afterwards finds it cached. Shown as plain text; a failed or empty
 * fetch shows nothing, since the peek's other lines stand on their own.
 */
function EmailPreview({ line }: { line: UpdateLine }) {
  const threadId = line.latest_thread_id;
  const thread = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => fetchThread(threadId ?? ""),
    enabled: Boolean(threadId),
    staleTime: 60_000,
    retry: false,
  });
  const body = thread.data?.bodies?.find(
    (candidate) => candidate.message_id === line.latest_message_id,
  );
  const lines = previewLines(previewSource(body));
  if (lines.length === 0) return null;
  return (
    <p
      data-testid="update-preview"
      className="type-reading max-w-[68ch] whitespace-pre-line text-text-1"
    >
      {lines.join("\n")}
    </p>
  );
}
