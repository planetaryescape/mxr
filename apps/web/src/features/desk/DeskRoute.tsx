import { Link } from "@tanstack/react-router";
import { Check } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  DESK_FULL_LANE_LIMIT,
  DESK_LANE_LIMIT,
  useDeskQuery,
  type Desk,
  type DeskElsewhere,
  type DeskLaneKind,
} from "./api";
import { deskHeadline, LANE_TITLES } from "./deskCopy";
import { deskDoneItem, markDeskDone, useDeskDone } from "./deskDone";
import { elsewhereLinks } from "./deskLinks";
import { deskGroups, partialLane } from "./deskRows";
import { DeskRow } from "./DeskRow";
import { LowTide } from "@/features/low-tide/LowTide";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { usePendingMailOps, type MailAction } from "@/features/mail-actions/pendingMailOps";
import type { InterceptedVerb } from "@/features/mail-actions/mailVerbs";
import { targetFromRows, type MailTarget } from "@/features/mail-actions/target";
import { ListWithReader } from "@/features/mailbox/ListWithReader";
import { dueDay, useNextPromise } from "@/features/promises/useNextPromise";
import type { RowRenderState } from "@/features/mailbox/MailboxList";
import type { RowAction } from "@/features/mailbox/MailboxRow";
import type { MessageRowView } from "@/features/mailbox/types";
import type { SwipeMap } from "@/features/swipe/swipeRules";
import { plural } from "@/lib/format";
import { useUiPrefs } from "@/state/uiPrefsStore";

/**
 * The desk: what needs you, not what arrived. Lanes come from the daemon
 * (`GetDesk`); the list, cursor, verbs and undo are the shared mail list's,
 * so `h`, `#` and `u` behave here exactly as in the inbox. `e` is Done:
 * put the conversation away (see deskDone). With a `lane`, the same page
 * shows that one lane in full.
 */
export function DeskRoute({ lane }: { lane?: DeskLaneKind }) {
  const account = useUiPrefs((s) => s.accountScope);
  // A lane's page starts with the first rows of each lane and can ask for
  // the whole lane; the choice resets when the lane changes.
  const [wholeLane, setWholeLane] = useState<DeskLaneKind | null>(null);
  const desk = useDeskQuery(lane && wholeLane === lane ? DESK_FULL_LANE_LIMIT : DESK_LANE_LIMIT);
  const ops = usePendingMailOps((s) => s.ops);
  const hidden = useDeskDone((s) => s.hidden);
  const { groups, index, byThread } = useMemo(
    () => deskGroups(desk.data, ops, lane, hidden),
    [desk.data, hidden, ops, lane],
  );
  // Only the whole desk has a low tide; a lane's page is just a list.
  const hasWork = groups.some((group) => group.rows.length > 0);
  const lowTide = useLowTide("desk", Boolean(lane) || !desk.data, hasWork);

  // The desk's rows by conversation, for the verbs below. They read it
  // through a ref so their identity never changes and the list's rows stay
  // memoized.
  const rowsByThread = useRef(byThread);
  useEffect(() => {
    rowsByThread.current = byThread;
  }, [byThread]);

  // On the desk, archive is Done: nothing for me to do here, put it away.
  // It covers the list, the selection and the reader opened from the desk.
  const interceptVerb = useCallback((action: MailAction, target: MailTarget): InterceptedVerb => {
    if (action !== "archive" && action !== "read-and-archive") return { rest: target };
    const threadIds = new Set(target.rows.map((row) => row.thread_id));
    if (target.threadId) threadIds.add(target.threadId);
    const onDesk = [...threadIds].flatMap((id) => {
      const row = rowsByThread.current.get(id);
      return row ? [row] : [];
    });
    if (onDesk.length === 0) return { rest: target };
    const rest = target.rows.filter((row) => !rowsByThread.current.has(row.thread_id));
    return {
      rest: rest.length > 0 ? targetFromRows(rest, target.source) : null,
      commit: () => void markDeskDone(onDesk.map(deskDoneItem)),
    };
  }, []);

  const doneRow = useCallback((row: MessageRowView) => {
    const source = rowsByThread.current.get(row.thread_id);
    if (source) void markDeskDone([deskDoneItem(source)]);
  }, []);

  // A short swipe right is Done, like `e`, in every lane; a long one
  // trashes, except on Waiting rows where there is nothing of yours to
  // trash. It runs through the archive verb, which the desk makes Done.
  const swipeActions = useCallback(
    (row: MessageRowView): SwipeMap =>
      index.get(row.id)?.lane === "waiting"
        ? { right: "done", left: "snooze" }
        : { right: "done", rightLong: "trash", left: "snooze" },
    [index],
  );

  const renderRow = useCallback(
    (row: MessageRowView, state: RowRenderState) => {
      const source = index.get(row.id);
      // Spread, not an object prop: the row stays memoized across renders.
      return source ? <DeskRow row={row} desk={source} onDone={doneRow} {...state} /> : null;
    },
    [doneRow, index],
  );

  // `w`, the list's row action, is Done too.
  const done = useMemo<RowAction>(
    () => ({
      label: "Done",
      icon: Check,
      describe: (row) => `Done with ${row.subject || "this conversation"}`,
      run: doneRow,
    }),
    [doneRow],
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
      swipeActions={swipeActions}
      rowAction={done}
      empty={<ClearDesk lane={lane} lowTide={lowTide} />}
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

function ClearDesk({ lane, lowTide }: { lane?: DeskLaneKind; lowTide: boolean }) {
  if (lowTide) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center px-6 py-12">
        <LowTide line="Low tide. Nobody's waiting on you." sound className="w-full">
          <NextDue />
          <p className="mt-3">
            New mail still arrives in the inbox: <kbd className="font-mono">g</kbd>{" "}
            <kbd className="font-mono">i</kbd>.
          </p>
        </LowTide>
      </div>
    );
  }
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

/** "Next thing due: Mon, notes to nora@…" when a promise of yours is open. */
function NextDue() {
  const next = useNextPromise();
  if (!next) return null;
  return (
    <p data-testid="low-tide-next">
      Next thing due: {dueDay(next.due)}, {next.commitment.what}
      {next.commitment.email ? ` to ${next.commitment.email}` : ""}.
    </p>
  );
}
