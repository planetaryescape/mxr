/*
 * The emails and people a step found or a reply names, drawn as the app's
 * own compact rows: avatar, subject or name, one line under it. Each opens
 * beside the assistant. The text is mail and model text, so it is only ever
 * set as text.
 */

import { Row } from "@/components/Row";
import { Avatar } from "@/components/Avatar";
import { linkHref, type AssistantLink } from "./links";
import { APP_LINK_ATTRIBUTE } from "./markdown";
import type { AssistantItem } from "./api";

type ReplyItem = Extract<AssistantItem, { kind: "assistant" }>;
export type ReadoutRow = NonNullable<ReplyItem["rows"]>[number];

/** Compact rhythm inside the panel (docs/blueprint/29 "The copy"). */
const COMPACT = { "--row-height": "44px" } as React.CSSProperties;

export function ReadoutRows({
  rows,
  more = 0,
  label,
}: {
  rows: ReadoutRow[];
  /** How many the step found beyond these. */
  more?: number;
  label: string;
}) {
  if (rows.length === 0) return null;
  return (
    <div className="-mx-3">
      <ul aria-label={label} style={COMPACT}>
        {rows.map((row, index) => (
          // oxlint-disable-next-line react/no-array-index-key -- rows have no id of their own and never reorder
          <ReadoutLine key={index} row={row} />
        ))}
      </ul>
      {more > 0 ? <p className="type-meta px-3 py-1 text-text-3">and {more} more</p> : null}
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
