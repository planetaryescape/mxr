import { Check, Star } from "lucide-react";
import { memo } from "react";

import type { DeskRow as DeskRowData } from "./api";
import { rowAge, rowPerson } from "./deskCopy";
import { GistAsk, gistText, gistTitle } from "@/features/gists/RowGistLine";
import { useRowGist } from "@/features/gists/rowGists";
import type { RowRenderState } from "@/features/mailbox/MailboxList";
import type { MessageRowView } from "@/features/mailbox/types";
import { cn } from "@/lib/utils";

type DeskRowProps = RowRenderState & {
  row: MessageRowView;
  desk: DeskRowData;
  /** Done: put the conversation away. Must be stable, the row is memoized. */
  onDone: (row: MessageRowView) => void;
  /**
   * A model is configured: reserve a line for the gist (what the
   * conversation is about) so nothing moves when it lands.
   */
  gistLine: boolean;
};

/**
 * Lanes where the other person's ask says why the row is here; the
 * daemon's `DeskLaneKind::shows_ask`, which the CLI and TUI use.
 */
const ASK_LANES = new Set(["owed", "people_new"]);

/**
 * One line of the desk: who, what and why, and how long. Wide lists read
 * as three columns; the narrow list beside the reader stacks the reason
 * under the person. No hover or focus transitions: the cursor moves on
 * every j and k, and movement there should be instant.
 *
 * Every row ends in a check, Done. With a mouse it shows on hover and on
 * the cursor's row; on touch it is always there, quiet. Its slot is always
 * reserved, so nothing shifts when it appears.
 */
export const DeskRow = memo(function DeskRow({
  row,
  desk,
  onDone,
  gistLine,
  ...state
}: DeskRowProps) {
  const who = rowPerson(desk);
  const age = rowAge(desk);
  const subject = desk.subject.trim();
  // This row's own gist: when it lands, only this row re-renders.
  const gist = useRowGist(desk.thread_id);
  // A row a time brought back keeps its reason: that is why it's here.
  const ask = gist?.ask && ASK_LANES.has(desk.lane) && !desk.back_at ? gist.ask : null;
  const askTitle = ask && gist ? gistTitle(gist) : undefined;
  return (
    <div
      id={state.domId}
      role="option"
      aria-selected={state.selected}
      aria-label={[
        row.starred ? "Starred." : null,
        who,
        subject || null,
        ask ? null : desk.reason,
        gist ? gistText(gist) : null,
        age.usual ? `${age.label}, ${age.usual}` : age.label,
        age.late ? "past the usual pace" : null,
      ]
        .filter(Boolean)
        .join(", ")}
      data-focused={state.focused ? "true" : undefined}
      data-lane={desk.lane}
      onClick={() => state.onOpen(row)}
      className={cn(
        "desk-row group/desk relative mx-2 grid cursor-default select-none rounded-md px-3 py-2",
        "grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-0.5",
        "@2xl:grid-cols-[minmax(7rem,11rem)_minmax(0,1fr)_auto] @2xl:items-baseline",
        state.selected
          ? "bg-primary-muted/70"
          : state.focused
            ? "bg-accent"
            : state.open
              ? "bg-accent/60"
              : "hover:bg-accent/40",
      )}
    >
      {state.focused || state.open ? (
        <span
          aria-hidden
          className={cn(
            "absolute inset-y-1.5 left-0 w-[2px] rounded-full",
            state.focused ? "bg-primary" : "bg-primary/45",
          )}
        />
      ) : null}
      <span
        className={cn(
          "min-w-0 truncate text-[13px]",
          desk.unread ? "font-semibold text-foreground" : "text-foreground/90",
        )}
        title={desk.counterparty_email}
      >
        {who}
      </span>
      <span
        className={cn(
          "col-span-2 col-start-1 row-start-2 min-w-0 text-[13px] text-muted-foreground @2xl:col-span-1 @2xl:col-start-2 @2xl:row-start-1 @2xl:truncate",
          // With a gist line the reason may turn into the ask: one line, so
          // the row keeps its height when it does.
          gistLine ? "truncate" : "line-clamp-2 @2xl:line-clamp-none",
        )}
        title={askTitle}
      >
        {/* On a narrow row an ask would be cut off after the subject: it
            goes first, and the gist line below says what it is about. */}
        {subject ? (
          <span className={cn("font-medium text-foreground/85", ask && "hidden @2xl:inline")}>
            {subject}
          </span>
        ) : null}
        {subject ? (
          <span aria-hidden className={cn(ask && "hidden @2xl:inline")}>
            {" "}
            ·{" "}
          </span>
        ) : null}
        {ask ? (
          <span data-testid="desk-ask">
            <GistAsk ask={ask} />
          </span>
        ) : (
          <span>{desk.reason}</span>
        )}
      </span>
      {gistLine ? (
        <span
          data-testid="desk-gist"
          data-state={gist ? "ready" : "waiting"}
          title={gist ? gistTitle(gist) : undefined}
          className="col-span-2 col-start-1 row-start-3 h-5 min-w-0 truncate text-[12.5px] leading-5 text-muted-foreground/90 @2xl:col-span-1 @2xl:col-start-2 @2xl:row-start-2"
        >
          {gist?.about ?? ""}
        </span>
      ) : null}
      <span className="col-start-2 row-start-1 flex items-center justify-end gap-1.5 @2xl:col-start-3">
        <time
          dateTime={desk.since}
          title={age.title}
          data-late={age.late ? "true" : undefined}
          className={cn(
            "whitespace-nowrap text-right font-mono text-2xs tabular-nums",
            age.late ? "text-warning" : "text-muted-foreground",
          )}
        >
          {row.starred ? (
            <Star
              aria-hidden
              className="mr-1.5 inline size-3 -translate-y-px fill-star text-star"
            />
          ) : null}
          {age.label}
          {age.usual ? <span className="text-muted-foreground"> · {age.usual}</span> : null}
        </time>
        {/* A span, not a button: rows are listbox options, which can't
            contain controls. The keyboard's Done is `e`. */}
        <span
          aria-hidden
          data-testid="desk-done"
          title="Done, put it away (e)"
          onClick={(event) => {
            event.stopPropagation();
            onDone(row);
          }}
          className={cn(
            "relative -my-1 grid size-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground",
            "hover:bg-primary-muted hover:text-primary",
            // On touch: a bigger target than it looks, so a tap on the
            // check never opens the row instead.
            "pointer-coarse:size-8 pointer-coarse:opacity-60 pointer-coarse:after:absolute pointer-coarse:after:-inset-1.5",
            state.focused || state.selected
              ? "pointer-fine:opacity-100"
              : "pointer-fine:opacity-0 pointer-fine:group-hover/desk:opacity-100",
          )}
        >
          <Check className="size-3.5" strokeWidth={2.25} />
        </span>
      </span>
    </div>
  );
});
