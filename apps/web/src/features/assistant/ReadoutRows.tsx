/*
 * The emails and people a step found or a reply names, drawn as the app's
 * own compact rows: avatar, subject or name, one line under it. Each opens
 * beside the assistant. The text is mail and model text, so it is only ever
 * set as text.
 */

import { Paperclip } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Row } from "@/components/Row";
import { Avatar } from "@/components/Avatar";
import { useThreadModesMap } from "@/features/modes/membership";
import { formatWhen } from "@/lib/format";
import { linkHref, type AssistantLink } from "./links";
import { LinkHoverCard } from "./LinkHoverCard";
import { APP_LINK_ATTRIBUTE } from "./markdown";
import { threadGlyph, type ModesByThread } from "./threadGlyph";
import type { AssistantItem, AssistantStepRows } from "./api";

type ReplyItem = Extract<AssistantItem, { kind: "assistant" }>;
export type ReadoutRow = NonNullable<ReplyItem["rows"]>[number];

/** Compact rhythm inside the panel (docs/blueprint/29 "The copy"). */
const COMPACT = { "--row-height": "44px" } as React.CSSProperties;

type Rest = "closed" | "loading" | "open" | "failed";

/**
 * A step's rows. When the step found more than these, "and N more" opens
 * every row the step's result lists; the first click fetches them. Without
 * `loadRest` the count stays plain text.
 *
 * The button is the same element in every state, so keyboard focus stays on
 * it while the rest loads and when it collapses. A response that arrives
 * after unmount is dropped, so a step that was swapped out never shows rows
 * fetched for it.
 */
export function ReadoutRows({
  rows,
  more = 0,
  label,
  loadRest,
}: {
  rows: ReadoutRow[];
  /** How many the step found beyond these. */
  more?: number;
  label: string;
  loadRest?: () => Promise<AssistantStepRows>;
}) {
  const [rest, setRest] = useState<Rest>("closed");
  const [all, setAll] = useState<AssistantStepRows | null>(null);
  const mounted = useRef(false);
  // Each click that starts a fetch takes a number; only the latest may land.
  const request = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      request.current += 1;
    };
  }, []);
  const open = rest === "open" && all !== null;
  const shown = open && all ? all.rows : rows;
  // One lookup for every email row shown. Rows from before the daemon sent
  // the email fields draw as they did and ask for nothing.
  const threadIds = useMemo(
    () =>
      shown.flatMap((row) =>
        isRich(row) && row.link?.kind === "thread" ? [row.link.thread_id] : [],
      ),
    [shown],
  );
  const modes = useThreadModesMap(threadIds).data;
  if (rows.length === 0) return null;
  const unloaded = all && all.total != null && all.total > all.listed ? all.total - all.listed : 0;

  const expand = () => {
    if (rest === "loading") return;
    if (rest === "open") {
      setRest("closed");
      return;
    }
    if (all) {
      setRest("open");
      return;
    }
    if (!loadRest) return;
    const id = ++request.current;
    setRest("loading");
    loadRest().then(
      (loaded) => {
        if (!mounted.current || id !== request.current) return;
        setAll(loaded);
        setRest("open");
      },
      () => {
        if (!mounted.current || id !== request.current) return;
        setRest("failed");
      },
    );
  };

  const loading = rest === "loading";
  return (
    <div className="-mx-3">
      <ul aria-label={label} style={COMPACT}>
        {shown.map((row, index) => (
          // oxlint-disable-next-line react/no-array-index-key -- rows have no id of their own and never reorder
          <ReadoutLine key={index} row={row} modes={modes} />
        ))}
      </ul>
      {more > 0 && loadRest ? (
        <>
          <button
            type="button"
            aria-expanded={open}
            aria-busy={loading}
            onClick={expand}
            className="type-meta px-3 py-1 text-text-2 outline-none hover:text-text-1 focus-visible:text-text-1"
          >
            {loading ? "Loading the rest…" : open ? "Show fewer" : `and ${more} more`}
          </button>
          {rest === "failed" ? (
            <p className="type-meta px-3 py-1 text-danger">{"Couldn't load the rest"}</p>
          ) : null}
        </>
      ) : more > 0 ? (
        <p className="type-meta px-3 py-1 text-text-3">and {more} more</p>
      ) : null}
      {open && unloaded > 0 ? (
        <p className="type-meta px-3 py-1 text-text-3">
          {`and ${unloaded} more matched that this step didn't load`}
        </p>
      ) : null}
    </div>
  );
}

/** Whether the daemon sent the email fields; older rows carry only `detail`. */
function isRich(row: ReadoutRow) {
  return Boolean(row.sender || row.snippet || row.at != null || row.attachment || row.unread);
}

function ReadoutLine({ row, modes }: { row: ReadoutRow; modes: ModesByThread }) {
  const link = row.link ?? null;
  const thread = link?.kind === "thread";
  const rich = isRich(row);
  // A thread row's title is the subject and its detail the sender; a person's is the other way round.
  const who = thread ? (row.sender ?? row.detail ?? row.title) : row.title;
  const under = [row.sender, row.snippet].filter(Boolean).join(" · ") || row.detail;
  const Glyph = thread && rich ? threadGlyph(link.thread_id, modes) : null;
  const marks =
    row.attachment || Glyph ? (
      <>
        {row.attachment ? (
          <Paperclip aria-label="Has an attachment" className="size-3.5" strokeWidth={1.75} />
        ) : null}
        {Glyph ? <Glyph aria-hidden className="size-3.5" strokeWidth={1.75} /> : null}
      </>
    ) : undefined;
  return (
    <Row
      lit={false}
      className="mx-0"
      unread={row.unread}
      lead={<Avatar name={who} size="sm" />}
      primary={row.title}
      secondary={under ?? undefined}
      trail={marks}
      time={row.at != null ? formatWhen(new Date(row.at * 1000)) : undefined}
      renderMain={
        link
          ? (content, className) => (
              <LinkHoverCard link={link} title={row.title} modes={modes}>
                <a
                  href={linkHref(link satisfies AssistantLink)}
                  {...{ [APP_LINK_ATTRIBUTE]: "" }}
                  className={`${className} focus-visible:text-signal`}
                >
                  {content}
                </a>
              </LinkHoverCard>
            )
          : undefined
      }
    />
  );
}
