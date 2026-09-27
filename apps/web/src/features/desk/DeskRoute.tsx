import { Link } from "@tanstack/react-router";
import { Check } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { toast } from "sonner";

import {
  DESK_FULL_LANE_LIMIT,
  DESK_LANE_LIMIT,
  useDeskQuery,
  type Desk,
  type DeskElsewhere,
  type DeskLaneKind,
} from "./api";
import { deskHeadline, LANE_TITLES } from "./deskCopy";
import { elsewhereLinks } from "./deskLinks";
import { deskGroups, partialLane } from "./deskRows";
import { markDoneWaiting, useDoneWaiting } from "./doneWaiting";
import { DeskRow } from "./DeskRow";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { usePendingMailOps, type MailAction } from "@/features/mail-actions/pendingMailOps";
import type { InterceptedVerb } from "@/features/mail-actions/mailVerbs";
import { targetFromRows, type MailTarget } from "@/features/mail-actions/target";
import { resolveCommitment } from "@/features/mailbox/api";
import { ListWithReader } from "@/features/mailbox/ListWithReader";
import type { RowRenderState } from "@/features/mailbox/MailboxList";
import type { RowAction } from "@/features/mailbox/MailboxRow";
import type { MessageRowView } from "@/features/mailbox/types";
import { plural } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";

/**
 * The desk: what needs you, not what arrived. Lanes come from the daemon
 * (`GetDesk`); the list, cursor, verbs and undo are the shared mail list's,
 * so `e`, `h`, `#` and `u` behave here exactly as in the inbox. With a
 * `lane`, the same page shows that one lane in full.
 */
export function DeskRoute({ lane }: { lane?: DeskLaneKind }) {
  const account = useUiPrefs((s) => s.accountScope);
  // A lane's page starts with the first rows of each lane and can ask for
  // the whole lane; the choice resets when the lane changes.
  const [wholeLane, setWholeLane] = useState<DeskLaneKind | null>(null);
  const desk = useDeskQuery(lane && wholeLane === lane ? DESK_FULL_LANE_LIMIT : DESK_LANE_LIMIT);
  const ops = usePendingMailOps((s) => s.ops);
  const hidden = useDoneWaiting((s) => s.hidden);
  const { groups, index } = useMemo(
    () => deskGroups(desk.data, ops, lane, hidden),
    [desk.data, hidden, ops, lane],
  );

  // Archive on a Waiting row means "done waiting": the thread you started
  // has nothing in the inbox to archive. Everything else archives as usual.
  const interceptVerb = useCallback(
    (action: MailAction, target: MailTarget): InterceptedVerb => {
      if (action !== "archive" && action !== "read-and-archive") return { rest: target };
      const isWaiting = (row: MessageRowView) => index.get(row.id)?.lane === "waiting";
      const waiting = target.rows.filter(isWaiting);
      if (waiting.length === 0) return { rest: target };
      const rest = target.rows.filter((row) => !isWaiting(row));
      return {
        rest: rest.length > 0 ? targetFromRows(rest, target.source) : null,
        commit: () => void markDoneWaiting(waiting.map((row) => row.thread_id)),
      };
    },
    [index],
  );

  const renderRow = useCallback(
    (row: MessageRowView, state: RowRenderState) => {
      const source = index.get(row.id);
      // Spread, not an object prop: the row stays memoized across renders.
      return source ? <DeskRow row={row} desk={source} {...state} /> : null;
    },
    [index],
  );

  // `w` is "done": a promise kept, or done waiting on a reply.
  const done = useMemo<RowAction>(
    () => ({
      label: "Done",
      icon: Check,
      describe: (row) => `Done with ${row.subject || "this conversation"}`,
      run: (row) => {
        const source = index.get(row.id);
        if (source?.lane === "waiting") {
          void markDoneWaiting([row.thread_id]);
          return;
        }
        const commitment = source?.commitment_id;
        if (!commitment) {
          toast.info("Done works on promises under Due and threads under Waiting on");
          return;
        }
        void resolveCommitment(commitment)
          .then(() => {
            toast.success("Promise kept");
            return invalidateMailQueries();
          })
          .catch((error: unknown) =>
            toast.error("Couldn't mark the promise done", {
              description: error instanceof Error ? error.message : String(error),
            }),
          );
      },
    }),
    [index],
  );

  const title = lane ? LANE_TITLES[lane] : "Desk";
  const laneTotal = lane && desk.data ? desk.data[lane].total : null;
  const partial = partialLane(desk.data, lane);
  return (
    <ListWithReader
      basePath="/desk"
      preserveSearch
      title={title}
      meta={laneTotal ? plural(laneTotal, "conversation") : null}
      actions={
        lane ? (
          <Link to="/desk" className="text-[12px] text-muted-foreground hover:text-foreground">
            Back to the desk
          </Link>
        ) : null
      }
      heading={!lane && desk.data ? <DeskHeading desk={desk.data} /> : undefined}
      toolbar={
        lane && partial ? (
          <p className="text-[12px] text-muted-foreground">
            Showing {partial.shown.toLocaleString()} of {partial.total.toLocaleString()}.{" "}
            <button
              type="button"
              onClick={() => setWholeLane(lane)}
              className="text-foreground underline decoration-border-strong underline-offset-4 hover:decoration-primary"
            >
              Show all
            </button>
          </p>
        ) : undefined
      }
      footer={!lane && desk.data ? <Elsewhere counts={desk.data.elsewhere} /> : null}
      groups={groups}
      scopeKey={`desk|${lane ?? "all"}|${account ?? "all"}`}
      status={desk}
      renderRow={renderRow}
      airyHeaders
      interceptVerb={interceptVerb}
      rowAction={done}
      empty={<ClearDesk lane={lane} />}
    />
  );
}

/** The greeting, whose counts double as links to each lane in full. */
function DeskHeading({ desk }: { desk: Desk }) {
  const headline = deskHeadline(desk);
  return (
    <div className="px-5 pb-3.5 pt-4">
      <h1 className="text-balance text-[17px] font-semibold tracking-tight text-foreground">
        {headline.lead}{" "}
        {headline.counts.length > 0 ? (
          <>
            {headline.counts.map((count, position) => (
              <span key={count.lane}>
                {position > 0 ? ", " : null}
                <Link
                  to="/desk"
                  search={{ lane: count.lane }}
                  className="underline decoration-border-strong decoration-1 underline-offset-4 hover:decoration-primary"
                >
                  {count.text}
                </Link>
              </span>
            ))}
            .
          </>
        ) : (
          <span className="font-normal text-muted-foreground">{headline.calm}</span>
        )}
      </h1>
      {headline.sub ? (
        <p className="mt-1 text-pretty text-[13px] text-muted-foreground">{headline.sub}</p>
      ) : null}
    </div>
  );
}

/** Everything that is not work, as one quiet line of links. */
function Elsewhere({ counts }: { counts: DeskElsewhere }) {
  const links = elsewhereLinks(counts);
  if (links.length === 0) return null;
  return (
    <nav
      aria-label="Everything else"
      className="flex shrink-0 flex-wrap items-baseline gap-x-4 gap-y-1 border-t border-border px-5 py-2.5 text-[12px]"
    >
      <span className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
        Everything else
      </span>
      {links.map(({ key, label, count, suffix, ...target }) => (
        <Link
          key={key}
          {...target}
          className="text-foreground/85 hover:text-foreground hover:underline"
        >
          {label}{" "}
          <span className="font-mono text-2xs tabular-nums text-muted-foreground">
            {count}
            {suffix ? ` ${suffix}` : ""}
          </span>
        </Link>
      ))}
    </nav>
  );
}

function ClearDesk({ lane }: { lane?: DeskLaneKind }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-1.5 px-8 py-16 text-center">
      <p className="text-[15px] text-foreground/90">
        {lane ? `Nothing under ${LANE_TITLES[lane]} right now.` : "The desk is clear."}
      </p>
      <p className="max-w-sm text-[13px] text-muted-foreground">
        New mail still arrives in the inbox, one key away: <kbd className="font-mono">g</kbd>{" "}
        <kbd className="font-mono">i</kbd>.
      </p>
    </div>
  );
}
