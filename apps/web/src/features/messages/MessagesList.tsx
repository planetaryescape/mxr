import { ChevronDown, ChevronRight, Users } from "lucide-react";

import { ArrivalGlow } from "@/components/ArrivalGlow";
import { Avatar, AvatarGlyph } from "@/components/Avatar";
import { AlsoInLine } from "@/features/modes/AlsoInLine";
import type { ThreadModes } from "@/features/modes/membership";
import { NewSenderQuestion } from "@/features/modes/NewSenderQuestion";
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
  quietOpen,
  onToggleQuiet,
  onSelect,
  memberships,
  hidden,
  arrival,
}: {
  data: MessagesData;
  selectedId: string | null;
  quietOpen: boolean;
  onToggleQuiet: () => void;
  onSelect: (id: string) => void;
  /** Which modes hold each row's first topic, for "Also in" and a new sender's question. */
  memberships?: ReadonlyMap<string, ThreadModes>;
  /** Threads whose done is on its way: no longer counted on their row. */
  hidden: ReadonlySet<string>;
  /** The row a verb just moved to, marked once. */
  arrival: Arrival | null;
}) {
  const modesOf = (row: MessagesRow) => {
    const thread = row.topics[0]?.thread_id;
    return thread ? memberships?.get(thread) : undefined;
  };
  return (
    <div data-testid="messages-list" className="pt-2">
      {data.your_turn.length > 0 ? (
        <Band id="band-your-turn" title="Your turn" testId="band-your_turn">
          {data.your_turn.map((row) => (
            <PersonRow
              key={row.id}
              row={row}
              modes={modesOf(row)}
              selected={row.id === selectedId}
              onSelect={onSelect}
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
                  className="relative flex flex-col items-center gap-1 rounded-md px-1.5 py-1 hover:bg-accent"
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
              modes={modesOf(row)}
              selected={row.id === selectedId}
              onSelect={onSelect}
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
                  modes={modesOf(row)}
                  selected={row.id === selectedId}
                  onSelect={onSelect}
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

function PersonRow({
  row,
  modes,
  selected,
  onSelect,
  hidden,
  arrival,
}: {
  row: MessagesRow;
  modes?: ThreadModes;
  selected: boolean;
  onSelect: (id: string) => void;
  hidden: ReadonlySet<string>;
  /** Set while this row is the one a verb just moved to. */
  arrival: number | null;
}) {
  const preview = row.preview;
  const topicCount = row.topics.filter(
    (topic) => topic.state !== "done" && !hidden.has(topic.thread_id),
  ).length;
  const subject = row.kind === "group" ? row.topics[0]?.subject : undefined;
  return (
    <li>
      <button
        type="button"
        data-testid="messages-row"
        data-row-id={row.id}
        data-band={row.band}
        aria-current={selected ? "true" : undefined}
        title={row.why}
        onClick={() => onSelect(row.id)}
        className={cn(
          "relative flex w-full items-start gap-3 px-4 py-2 text-left",
          selected ? "bg-accent" : "hover:bg-accent/60",
        )}
      >
        {arrival !== null ? <ArrivalGlow key={arrival} /> : null}
        <Face title={row.title} group={row.kind === "group"} />
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline gap-2">
            <span
              data-testid="row-title"
              className={cn(
                "min-w-0 flex-1 truncate text-sm",
                row.your_turn ? "font-semibold" : "font-medium",
              )}
            >
              {row.title}
            </span>
            {topicCount > 1 ? (
              <span
                data-testid="row-topic-count"
                className="shrink-0 text-xs tabular-nums text-muted-foreground"
              >
                {topicCount} topics
              </span>
            ) : null}
            <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
              {rowTime(row)}
            </span>
            {row.your_turn ? (
              <StatusDot
                aria-hidden={false}
                aria-label="Your turn"
                className="size-2 self-center bg-primary"
              />
            ) : null}
          </span>
          {subject ? (
            <span className="block truncate text-xs text-muted-foreground">{subject}</span>
          ) : null}
          {preview ? (
            <span
              data-testid="row-preview"
              data-kind={preview.kind}
              title={
                preview.kind === "ask" && preview.model
                  ? `The ask, from a summary by ${preview.model}`
                  : undefined
              }
              className="mt-0.5 line-clamp-2 text-xs text-muted-foreground"
            >
              {preview.kind === "ask" ? `“${preview.text}”` : preview.text}
            </span>
          ) : null}
          <AlsoInLine modes={modes} here="messages" keys={false} links={false} className="mt-0.5" />
        </span>
      </button>
      {modes?.new_sender ? (
        <div className="px-4 pb-2 pl-[3.75rem]">
          <NewSenderQuestion question={modes.new_sender} label={row.title} />
        </div>
      ) : null}
    </li>
  );
}
