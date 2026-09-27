import { Link } from "@tanstack/react-router";
import { Check } from "lucide-react";
import { useCallback, useMemo } from "react";
import { toast } from "sonner";

import { useDeskQuery, type Desk, type DeskElsewhere, type DeskLaneKind } from "./api";
import { deskHeadline, LANE_TITLES } from "./deskCopy";
import { deskGroups } from "./deskRows";
import { DeskRow } from "./DeskRow";
import { invalidateMailQueries } from "@/features/mail-actions/mailMutations";
import { usePendingMailOps } from "@/features/mail-actions/pendingMailOps";
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
  const desk = useDeskQuery();
  const ops = usePendingMailOps((s) => s.ops);
  const { groups, index } = useMemo(() => deskGroups(desk.data, ops, lane), [desk.data, ops, lane]);

  const renderRow = useCallback(
    (row: MessageRowView, state: RowRenderState) => {
      const source = index.get(row.id);
      return source ? <DeskRow row={row} desk={source} state={state} /> : null;
    },
    [index],
  );

  // `w` marks a promise kept. Other rows have no row verb of their own.
  const done = useMemo<RowAction>(
    () => ({
      label: "Done",
      icon: Check,
      describe: (row) => `Mark the promise in ${row.subject || "this conversation"} done`,
      run: (row) => {
        const commitment = index.get(row.id)?.commitment_id;
        if (!commitment) {
          toast.info("Only promises under Due can be marked done");
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
      footer={!lane && desk.data ? <Elsewhere counts={desk.data.elsewhere} /> : null}
      groups={groups}
      scopeKey={`desk|${lane ?? "all"}|${account ?? "all"}`}
      status={desk}
      renderRow={renderRow}
      airyHeaders
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

const ELSEWHERE: { key: keyof DeskElsewhere; label: string; to: string; suffix?: string }[] = [
  { key: "reading", label: "Reading", to: "/subscriptions", suffix: "new" },
  // No paper-trail view yet (bundles arrive in a later release); receipts
  // and notifications sit in the inbox, so that is where this points.
  { key: "paper_trail", label: "Paper trail", to: "/m/inbox" },
  { key: "deliveries", label: "Deliveries", to: "/deliveries" },
  { key: "invites", label: "Invites", to: "/invites" },
  { key: "screener", label: "Screener", to: "/screener" },
];

/** Everything that is not work, as one quiet line of links. */
function Elsewhere({ counts }: { counts: DeskElsewhere }) {
  const items = ELSEWHERE.filter((item) => counts[item.key] > 0);
  if (items.length === 0) return null;
  return (
    <nav
      aria-label="Everything else"
      className="flex shrink-0 flex-wrap items-baseline gap-x-4 gap-y-1 border-t border-border px-5 py-2.5 text-[12px]"
    >
      <span className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
        Everything else
      </span>
      {items.map((item) => (
        <Link
          key={item.key}
          to={item.to}
          className="text-foreground/85 hover:text-foreground hover:underline"
        >
          {item.label}{" "}
          <span className="font-mono text-2xs tabular-nums text-muted-foreground">
            {counts[item.key]}
            {item.suffix ? ` ${item.suffix}` : ""}
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
