/*
 * The emails and people a step found or a reply names, drawn as the app's
 * own compact rows: avatar, subject or name, one line under it. Each opens
 * beside the assistant. The text is mail and model text, so it is only ever
 * set as text.
 */

import { useState } from "react";

import { Row } from "@/components/Row";
import { Avatar } from "@/components/Avatar";
import { linkHref, type AssistantLink } from "./links";
import { APP_LINK_ATTRIBUTE } from "./markdown";
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
  if (rows.length === 0) return null;

  const open = rest === "open" && all !== null;
  const shown = open ? all.rows : rows;
  const unloaded = all && all.total != null && all.total > all.listed ? all.total - all.listed : 0;

  const expand = () => {
    if (rest === "open") {
      setRest("closed");
      return;
    }
    if (all) {
      setRest("open");
      return;
    }
    if (!loadRest) return;
    setRest("loading");
    loadRest().then(
      (loaded) => {
        setAll(loaded);
        setRest("open");
      },
      () => setRest("failed"),
    );
  };

  return (
    <div className="-mx-3">
      <ul aria-label={label} style={COMPACT}>
        {shown.map((row, index) => (
          // oxlint-disable-next-line react/no-array-index-key -- rows have no id of their own and never reorder
          <ReadoutLine key={index} row={row} />
        ))}
      </ul>
      {more > 0 && loadRest ? (
        <>
          {rest === "loading" ? (
            <p className="type-meta px-3 py-1 text-text-3" role="status">
              Loading the rest…
            </p>
          ) : (
            <button
              type="button"
              aria-expanded={open}
              onClick={expand}
              className="type-meta px-3 py-1 text-text-2 outline-none hover:text-text-1 focus-visible:text-text-1"
            >
              {open ? "Show fewer" : `and ${more} more`}
            </button>
          )}
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

function ReadoutLine({ row }: { row: ReadoutRow }) {
  const link = row.link ?? null;
  // A thread row's title is the subject and its detail the sender; a person's is the other way round.
  const who = link?.kind === "thread" ? (row.detail ?? row.title) : row.title;
  return (
    <Row
      lit={false}
      className="mx-0"
      lead={<Avatar name={who} size="sm" />}
      primary={row.title}
      secondary={row.detail ?? undefined}
      renderMain={
        link
          ? (content, className) => (
              <a
                href={linkHref(link satisfies AssistantLink)}
                {...{ [APP_LINK_ATTRIBUTE]: "" }}
                className={`${className} focus-visible:text-signal`}
              >
                {content}
              </a>
            )
          : undefined
      }
    />
  );
}
