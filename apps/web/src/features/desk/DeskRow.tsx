import { Star } from "lucide-react";
import { memo } from "react";

import type { DeskRow as DeskRowData } from "./api";
import { rowAge, rowPerson } from "./deskCopy";
import type { RowRenderState } from "@/features/mailbox/MailboxList";
import type { MessageRowView } from "@/features/mailbox/types";
import { cn } from "@/lib/utils";

type DeskRowProps = RowRenderState & {
  row: MessageRowView;
  desk: DeskRowData;
};

/**
 * One line of the desk: who, what and why, and how long. Wide lists read
 * as three columns; the narrow list beside the reader stacks the reason
 * under the person. No hover or focus transitions: the cursor moves on
 * every j and k, and movement there should be instant.
 */
export const DeskRow = memo(function DeskRow({ row, desk, ...state }: DeskRowProps) {
  const who = rowPerson(desk);
  const age = rowAge(desk);
  const subject = desk.subject.trim();
  return (
    <div
      id={state.domId}
      role="option"
      aria-selected={state.selected}
      aria-label={[
        row.starred ? "Starred." : null,
        who,
        subject || null,
        desk.reason,
        age.usual ? `${age.label}, ${age.usual}` : age.label,
        age.late ? "past the usual pace" : null,
      ]
        .filter(Boolean)
        .join(", ")}
      data-focused={state.focused ? "true" : undefined}
      data-lane={desk.lane}
      onClick={() => state.onOpen(row)}
      className={cn(
        "desk-row relative mx-2 grid cursor-default select-none rounded-md px-3 py-2",
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
      <span className="col-span-2 col-start-1 row-start-2 line-clamp-2 min-w-0 text-[13px] text-muted-foreground @2xl:col-span-1 @2xl:col-start-2 @2xl:row-start-1 @2xl:line-clamp-none @2xl:truncate">
        {subject ? <span className="font-medium text-foreground/85">{subject}</span> : null}
        {subject ? <span aria-hidden> · </span> : null}
        <span>{desk.reason}</span>
      </span>
      <time
        dateTime={desk.since}
        title={age.title}
        className={cn(
          "col-start-2 row-start-1 whitespace-nowrap text-right font-mono text-2xs tabular-nums @2xl:col-start-3",
          age.late ? "text-warning" : "text-muted-foreground",
        )}
      >
        {row.starred ? (
          <Star aria-hidden className="mr-1.5 inline size-3 -translate-y-px fill-star text-star" />
        ) : null}
        {age.label}
        {age.usual ? <span className="text-muted-foreground"> · {age.usual}</span> : null}
      </time>
    </div>
  );
});
