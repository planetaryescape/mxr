import { ChevronDown, ChevronRight, Users } from "lucide-react";
import { memo } from "react";

import { ArrivalGlow } from "@/components/ArrivalGlow";
import { Avatar, AvatarGlyph } from "@/components/Avatar";
import { ModeMarks, NewMark } from "@/components/ModeMark";
import { Row, RowVerb, type LampVia } from "@/components/Row";
import { otherModes, type ThreadModes } from "@/features/modes/membership";
import { NewSenderChips } from "@/features/modes/NewSenderChips";
import { cn } from "@/lib/utils";

import type { MessagesData, MessagesRow } from "./api";
import { rowTime, type Arrival } from "./messagesView";
import { SectionHeading } from "@/components/SectionHeading";
import { StatusDot } from "@/components/StatusDot";

/**
 * The people, in the daemon's bands: Your turn (closest first, then most
 * overdue), Pinned as a strip of faces, Recent with "You: …" when you wrote
 * last, and Quiet folded with its count. The row is a person; the preview
 * is what they asked; the state is the turn, never read or unread.
 */
export function MessagesList({
  data,
  selectedId,
  via,
  quietOpen,
  onToggleQuiet,
  onSelect,
  onReply,
  onDone,
  memberships,
  hidden,
  arrival,
}: {
  data: MessagesData;
  selectedId: string | null;
  via: LampVia;
  quietOpen: boolean;
  onToggleQuiet: () => void;
  /** Must be stable (not re-created per render), so a lamp move leaves other rows alone. */
  onSelect: (id: string) => void;
  onReply: () => void;
  onDone: () => void;
  /** Which modes hold each row's first topic, for "Also in" and a new sender's question. */
  memberships?: ReadonlyMap<string, ThreadModes>;
  /** Threads whose done is on its way: no longer counted on their row. */
  hidden: ReadonlySet<string>;
  /** The row a verb just moved to, marked once. */
  arrival: Arrival | null;
}) {
  return (
    <div data-testid="messages-list" className="pt-2">
      {data.your_turn.length > 0 ? (
        <Band id="band-your-turn" title="Your turn" testId="band-your_turn">
          {data.your_turn.map((row) => (
            <PersonRow
              key={row.id}
              row={row}
              memberships={memberships}
              lit={row.id === selectedId}
              via={via}
              onSelect={onSelect}
              onReply={onReply}
              onDone={onDone}
              hidden={hidden}
              arrival={arrival?.person === row.id ? arrival.key : null}
            />
          ))}
        </Band>
      ) : null}
      {data.pinned.length > 0 ? (
        <section aria-labelledby="band-pinned" data-testid="band-pinned" className="mt-3">
          <SectionHeading id="band-pinned" className="px-5 pb-1">
            Pinned
          </SectionHeading>
          <ul className="flex flex-wrap gap-2 px-4">
            {data.pinned.map((row) => (
              <li key={row.id}>
                <button
                  type="button"
                  data-testid="pinned-face"
                  data-row-id={row.id}
                  onClick={() => onSelect(row.id)}
                  aria-label={`${row.title}${row.your_turn ? ", your turn" : ""}`}
                  aria-current={row.id === selectedId ? "true" : undefined}
                  className={cn(
                    "relative flex flex-col items-center gap-1 rounded-md px-1.5 py-1 hover:bg-ink-lamp",
                    row.id === selectedId && "bg-ink-lamp",
                  )}
                >
                  <Face title={row.title} />
                  <span className="max-w-[4.5rem] truncate text-2xs text-foreground/85">
                    {row.title.split(/\s+/)[0]}
                  </span>
                  {row.your_turn ? (
                    <StatusDot className="absolute right-1 top-1 size-2 bg-primary" />
                  ) : null}
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {data.recent.length > 0 ? (
        <Band id="band-recent" title="Recent" testId="band-recent">
          {data.recent.map((row) => (
            <PersonRow
              key={row.id}
              row={row}
              memberships={memberships}
              lit={row.id === selectedId}
              via={via}
              onSelect={onSelect}
              onReply={onReply}
              onDone={onDone}
              hidden={hidden}
              arrival={arrival?.person === row.id ? arrival.key : null}
            />
          ))}
          {data.recent_total > data.recent.length ? (
            <li className="px-5 py-1 text-xs text-muted-foreground">
              and {data.recent_total - data.recent.length} more
            </li>
          ) : null}
        </Band>
      ) : null}
      {data.quiet_total > 0 ? (
        <section aria-label="Quiet" data-testid="band-quiet" className="mt-3">
          <button
            type="button"
            onClick={onToggleQuiet}
            aria-expanded={quietOpen}
            className="flex w-full items-center gap-1 px-5 py-1 text-left text-2xs font-medium uppercase tracking-wide text-muted-foreground hover:text-foreground"
          >
            {quietOpen ? (
              <ChevronDown className="size-3" aria-hidden />
            ) : (
              <ChevronRight className="size-3" aria-hidden />
            )}
            Quiet ({data.quiet_total})
          </button>
          {quietOpen ? (
            <ul className="grid grid-cols-[minmax(0,1fr)]">
              {data.quiet.map((row) => (
                <PersonRow
                  key={row.id}
                  row={row}
                  memberships={memberships}
                  lit={row.id === selectedId}
                  via={via}
                  onSelect={onSelect}
                  onReply={onReply}
                  onDone={onDone}
                  hidden={hidden}
                  arrival={arrival?.person === row.id ? arrival.key : null}
                />
              ))}
            </ul>
          ) : null}
        </section>
      ) : null}
    </div>
  );
}

function Band({
  id,
  title,
  testId,
  children,
}: {
  id: string;
  title: string;
  testId: string;
  children: React.ReactNode;
}) {
  return (
    <section aria-labelledby={id} data-testid={testId} className="mt-1">
      <SectionHeading id={id} className="px-5 pb-1 pt-2">
        {title}
      </SectionHeading>
      <ul className="grid grid-cols-[minmax(0,1fr)]">{children}</ul>
    </section>
  );
}

function Face({ title, group = false }: { title: string; group?: boolean }) {
  // A group has no one sender to colour, so it gets the neutral people glyph.
  if (!group) return <Avatar name={title} size="lg" />;
  return (
    <AvatarGlyph size="lg">
      <Users className="size-3.5" />
    </AvatarGlyph>
  );
}

/**
 * One person. Memoised on primitives and stable callbacks, so moving the lamp
 * re-renders the row that lost it and the row that gained it, not the list
 * (spike S4, docs/blueprint/31).
 */
const PersonRow = memo(function PersonRow({
  row,
  memberships,
  lit,
  via,
  onSelect,
  onReply,
  onDone,
  hidden,
  arrival,
}: {
  row: MessagesRow;
  /** Looked up here, not by the list, so a lamp move doesn't walk every row. */
  memberships?: ReadonlyMap<string, ThreadModes>;
  /** The lamp is on this row: its person page is the one open. */
  lit: boolean;
  via: LampVia;
  onSelect: (id: string) => void;
  onReply: () => void;
  onDone: () => void;
  hidden: ReadonlySet<string>;
  /** Set while this row is the one a verb just moved to. */
  arrival: number | null;
}) {
  const preview = row.preview;
  const modes = memberships?.get(row.topics[0]?.thread_id ?? "");
  const topicCount = row.topics.filter(
    (topic) => topic.state !== "done" && !hidden.has(topic.thread_id),
  ).length;
  const subject = row.kind === "group" ? row.topics[0]?.subject : undefined;
  const newSender = modes?.new_sender;
  const secondary = preview
    ? preview.kind === "ask"
      ? `\u201c${preview.text}\u201d`
      : preview.text
    : subject;
  return (
    <Row
      lit={lit}
      via={via}
      unread={row.unread}
      onClick={() => onSelect(row.id)}
      lead={<Face title={row.title} group={row.kind === "group"} />}
      primary={
        <>
          <span
            data-testid="row-title"
            className={cn(row.your_turn || row.unread ? "font-semibold" : undefined)}
          >
            {row.title}
          </span>
          {topicCount > 1 ? (
            <span data-testid="row-topic-count" className="type-meta ml-2 tabular-nums text-text-3">
              {topicCount} topics
            </span>
          ) : null}
          {newSender ? <NewMark className="ml-2" /> : null}
        </>
      }
      secondary={
        secondary ? (
          <span
            data-testid="row-preview"
            data-kind={preview?.kind}
            title={
              preview?.kind === "ask" && preview.model
                ? `The ask, from a summary by ${preview.model}`
                : undefined
            }
          >
            {secondary}
          </span>
        ) : undefined
      }
      trail={
        <>
          <ModeMarks modes={otherModes(modes, "messages")} />
          <span className="whitespace-nowrap tabular-nums">{rowTime(row)}</span>
        </>
      }
      verbs={
        newSender ? (
          <NewSenderChips question={newSender} label={row.title} />
        ) : (
          <>
            <RowVerb keys="r" label="reply" ariaLabel={`Reply to ${row.title}`} onClick={onReply} />
            <RowVerb keys="e" label="done here" ariaLabel="Done here (e)" onClick={onDone} />
          </>
        )
      }
      renderMain={(content, className) => (
        <button
          type="button"
          data-testid="messages-row"
          data-row-id={row.id}
          data-band={row.band}
          aria-current={lit ? "true" : undefined}
          title={row.why}
          onClick={(event) => {
            event.stopPropagation();
            onSelect(row.id);
          }}
          className={cn(className, "focus-visible:ring-2 focus-visible:ring-signal")}
        >
          {arrival !== null ? <ArrivalGlow key={arrival} /> : null}
          {content}
        </button>
      )}
    />
  );
});
