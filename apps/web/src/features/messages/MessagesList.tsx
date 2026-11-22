import { ChevronDown, ChevronRight, Users } from "lucide-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { ArrivalGlow } from "@/components/ArrivalGlow";
import { Avatar, AvatarGlyph } from "@/components/Avatar";
import { ModeMarks, NewMark } from "@/components/ModeMark";
import { modelMarkTitle } from "@/components/ModelMark";
import { Row, RowVerb, type LampVia } from "@/components/Row";
import { useModeDone } from "@/features/modes/modeDone";
import { otherModes, type ThreadModes } from "@/features/modes/membership";
import { NewSenderChips } from "@/features/modes/NewSenderChips";
import { cn } from "@/lib/utils";

import type { MessagesData, MessagesRow } from "./api";
import { rowTime, type Arrival } from "./messagesView";
import { SectionHeading } from "@/components/SectionHeading";
import { StatusDot } from "@/components/StatusDot";

export function MessagesList({
  data,
  selectedId,
  via,
  quietOpen,
  onToggleQuiet,
  scroller,
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
  scroller: HTMLElement | null;
  onSelect: (id: string) => void;
  onReply: () => void;
  onDone: () => void;
  memberships?: ReadonlyMap<string, ThreadModes>;
  hidden: ReadonlySet<string>;
  arrival: Arrival | null;
}) {
  const renderPerson = (row: MessagesRow, slot?: RowSlot) => (
    <PersonRow
      key={row.id}
      row={row}
      slotIndex={slot?.index}
      slotTop={slot?.top}
      setSize={slot?.setSize}
      measure={slot?.measure}
      memberships={memberships}
      lit={row.id === selectedId}
      via={via}
      onSelect={onSelect}
      onReply={onReply}
      onDone={onDone}
      hidden={hidden}
      arrival={arrival?.person === row.id ? arrival.key : null}
    />
  );
  return (
    <div data-testid="messages-list" className="pt-2">
      {data.your_turn.length > 0 ? (
        <Band id="band-your-turn" title="Your turn" testId="band-your_turn">
          <VirtualRows
            rows={data.your_turn}
            selectedId={selectedId}
            scroller={scroller}
            renderRow={renderPerson}
          />
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
          <VirtualRows
            rows={data.recent}
            selectedId={selectedId}
            scroller={scroller}
            renderRow={renderPerson}
          />
          {data.recent_total > data.recent.length ? (
            <p className="px-5 py-1 text-xs text-muted-foreground">
              and {data.recent_total - data.recent.length} more
            </p>
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
            <>
              <VirtualRows
                rows={data.quiet}
                selectedId={selectedId}
                scroller={scroller}
                renderRow={renderPerson}
              />
            </>
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
      {children}
    </section>
  );
}

const ROW_ESTIMATE_PX = 52;
const OVERSCAN = 12;

function VirtualRows({
  rows,
  selectedId,
  scroller,
  renderRow,
}: {
  rows: readonly MessagesRow[];
  selectedId: string | null;
  scroller: HTMLElement | null;
  renderRow: (row: MessagesRow, slot: RowSlot) => ReactNode;
}) {
  const listRef = useRef<HTMLUListElement>(null);
  const [margin, setMargin] = useState(0);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useLayoutEffect(() => {
    const element = scroller;
    const list = listRef.current;
    if (!element || !list) return;
    const top =
      list.getBoundingClientRect().top - element.getBoundingClientRect().top + element.scrollTop;
    setMargin((current) => (Math.abs(current - top) < 1 ? current : Math.round(top)));
  });
  const getItemKey = useCallback((index: number) => rows[index]?.id ?? index, [rows]);
  const indexById = useMemo(() => new Map(rows.map((row, index) => [row.id, index])), [rows]);
  const litIndex = selectedId ? indexById.get(selectedId) : undefined;
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller,
    estimateSize: () => ROW_ESTIMATE_PX,
    overscan: OVERSCAN,
    getItemKey,
    scrollMargin: margin,
  });
  useEffect(() => {
    if (litIndex !== undefined) virtualizer.scrollToIndex(litIndex, { align: "auto" });
  }, [litIndex, virtualizer]);
  const windowed = virtualizer.getVirtualItems();
  const litItem =
    litIndex !== undefined && !windowed.some((item) => item.index === litIndex)
      ? virtualizer.measurementsCache[litIndex]
      : undefined;
  return (
    <ul ref={listRef} style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
      {(litItem ? [...windowed, litItem] : windowed).map((item) => {
        const row = rows[item.index];
        if (!row) return null;
        return renderRow(row, {
          index: item.index,
          top: item.start - margin,
          setSize: rows.length,
          measure: virtualizer.measureElement,
        });
      })}
    </ul>
  );
}

export interface RowSlot {
  index: number;
  top: number;
  setSize: number;
  measure: (element: Element | null) => void;
}

function Face({ title, group = false }: { title: string; group?: boolean }) {
  if (!group) return <Avatar name={title} size="lg" />;
  return (
    <AvatarGlyph size="lg">
      <Users className="size-3.5" />
    </AvatarGlyph>
  );
}

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
  slotIndex,
  slotTop,
  setSize,
  measure,
}: {
  row: MessagesRow;
  memberships?: ReadonlyMap<string, ThreadModes>;
  lit: boolean;
  via: LampVia;
  onSelect: (id: string) => void;
  onReply: () => void;
  onDone: () => void;
  hidden: ReadonlySet<string>;
  arrival: number | null;
  slotIndex?: number;
  slotTop?: number;
  setSize?: number;
  measure?: (element: Element | null) => void;
}) {
  const preview = row.preview;
  const modes = memberships?.get(row.topics[0]?.thread_id ?? "");
  const topicCount = row.topics.filter(
    (topic) => topic.state !== "done" && !hidden.has(topic.thread_id),
  ).length;
  const allGone =
    row.topics.some((topic) => hidden.has(topic.thread_id)) &&
    row.topics.every((topic) => topic.state === "done" || hidden.has(topic.thread_id));
  const folding = useModeDone((s) => row.topics.some((t) => s.folding.messages.has(t.thread_id)));
  const settle = useModeDone((s) => s.settle);
  const landIds = useMemo(
    () => row.topics.flatMap((topic) => [topic.thread_id, ...topic.message_ids.slice(-20)]),
    [row.topics],
  );
  const subject = row.topics[0]?.subject;
  const newSender = modes?.new_sender;
  const secondary = preview
    ? preview.kind === "ask"
      ? `\u201c${preview.text}\u201d`
      : preview.text
    : subject;
  if (allGone && !folding) return null;
  return (
    <Row
      accountId={row.account_id}
      ref={measure}
      data-index={slotIndex}
      aria-setsize={setSize}
      aria-posinset={slotIndex === undefined ? undefined : slotIndex + 1}
      className={slotTop === undefined ? undefined : "absolute inset-x-0 top-0"}
      style={slotTop === undefined ? undefined : { transform: `translateY(${slotTop}px)` }}
      lit={lit}
      via={via}
      unread={row.unread}
      leaving={allGone}
      land={landIds}
      onAnimationEnd={
        allGone
          ? (event) => {
              if (event.target !== event.currentTarget) return;
              settle(
                "messages",
                row.topics.map((topic) => topic.thread_id),
              );
            }
          : undefined
      }
      onClick={() => onSelect(row.id)}
      lead={<Face title={row.title} group={row.kind === "group"} />}
      primary={
        <>
          <span data-testid="row-title">{row.title}</span>
          {topicCount > 1 ? (
            <span data-testid="row-topic-count" className="type-meta ml-2 tabular-nums text-text-3">
              {topicCount} topics
            </span>
          ) : null}
          {newSender ? <NewMark className="ml-2" /> : null}
        </>
      }
      secondary={
        row.draft ? (
          <span data-testid="row-preview" data-kind="draft">
            <span className="font-medium text-signal">Draft:</span> {row.draft.text}
          </span>
        ) : secondary ? (
          <span
            data-testid="row-preview"
            data-kind={preview?.kind}
            title={
              preview?.kind === "ask" && preview.model
                ? `The ask. ${modelMarkTitle(preview.model)}`
                : undefined
            }
          >
            {secondary}
          </span>
        ) : undefined
      }
      trail={<ModeMarks modes={otherModes(modes, "messages")} />}
      time={<span className="whitespace-nowrap tabular-nums">{rowTime(row)}</span>}
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
