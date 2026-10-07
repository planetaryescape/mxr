import { ChevronDown, ChevronRight, Users } from "lucide-react";

import { AlsoInLine } from "@/features/modes/AlsoInLine";
import type { ThreadModes } from "@/features/modes/membership";
import { NewSenderQuestion } from "@/features/modes/NewSenderQuestion";
import { cn } from "@/lib/utils";

import type { MessagesData, MessagesRow } from "./api";
import { initials, rowTime } from "./messagesView";

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
}: {
  data: MessagesData;
  selectedId: string | null;
  quietOpen: boolean;
  onToggleQuiet: () => void;
  onSelect: (id: string) => void;
  /** Which modes hold each row's first topic, for "Also in" and a new sender's question. */
  memberships?: ReadonlyMap<string, ThreadModes>;
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
            />
          ))}
        </Band>
      ) : null}
      {data.pinned.length > 0 ? (
        <section aria-labelledby="band-pinned" data-testid="band-pinned" className="mt-3">
          <h2 id="band-pinned" className="px-5 pb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            Pinned
          </h2>
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
                  <span className="max-w-[4.5rem] truncate text-[11.5px] text-foreground/85">
                    {row.title.split(/\s+/)[0]}
                  </span>
                  {row.your_turn ? (
                    <span aria-hidden className="absolute right-1 top-1 size-2 rounded-full bg-primary" />
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
            />
          ))}
          {data.recent_total > data.recent.length ? (
            <li className="px-5 py-1 text-[12px] text-muted-foreground">
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
            className="flex w-full items-center gap-1 px-5 py-1 text-left text-[11px] font-medium uppercase tracking-wide text-muted-foreground hover:text-foreground"
          >
            {quietOpen ? <ChevronDown className="size-3" aria-hidden /> : <ChevronRight className="size-3" aria-hidden />}
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
      <h2 id={id} className="px-5 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {title}
      </h2>
      <ul className="grid grid-cols-[minmax(0,1fr)]">{children}</ul>
    </section>
  );
}

function Face({ title, group = false }: { title: string; group?: boolean }) {
  return (
    <span
      aria-hidden
      className="grid size-8 shrink-0 place-items-center rounded-full bg-muted text-[11.5px] font-medium text-foreground/80"
    >
      {group ? <Users className="size-3.5" /> : initials(title)}
    </span>
  );
}

function PersonRow({
  row,
  modes,
  selected,
  onSelect,
}: {
  row: MessagesRow;
  modes?: ThreadModes;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  const preview = row.preview;
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
          "flex w-full items-start gap-3 px-4 py-2 text-left",
          selected ? "bg-accent" : "hover:bg-accent/60",
        )}
      >
        <Face title={row.title} group={row.kind === "group"} />
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline gap-2">
            <span data-testid="row-title" className={cn("min-w-0 flex-1 truncate text-[13.5px]", row.your_turn ? "font-semibold" : "font-medium")}>
              {row.title}
            </span>
            <span className="shrink-0 text-[12px] tabular-nums text-muted-foreground">{rowTime(row)}</span>
            {row.your_turn ? (
              <span aria-label="Your turn" className="size-2 shrink-0 self-center rounded-full bg-primary" />
            ) : null}
          </span>
          {subject ? <span className="block truncate text-[12.5px] text-muted-foreground">{subject}</span> : null}
          {preview ? (
            <span
              data-testid="row-preview"
              data-kind={preview.kind}
              title={preview.kind === "ask" && preview.model ? `The ask, from a summary by ${preview.model}` : undefined}
              className="mt-0.5 line-clamp-2 text-[12.5px] text-muted-foreground"
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
