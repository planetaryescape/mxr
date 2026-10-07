import { useParams } from "@tanstack/react-router";
import { Bell, ChevronRight, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { useReaderNav } from "@/features/mailbox/readerNav";
import { useModeGuide, useRetireCard, type ModeGuide } from "@/features/modes/api";
import { ModeCard } from "@/features/modes/ModeCard";
import { ModeFrame, ModeHeader } from "@/components/ModeFrame";
import { PlaceLayout } from "@/features/places/PlaceLayout";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { useDigest, type UpdateLine, type UpdateSection, type UpdatesDigest } from "./api";
import {
  cutLine,
  digestRows,
  openableLink,
  SECTION_TITLE,
  canLetGoSource,
  canTune,
  type SuggestedSetting,
} from "./digestView";
import { LetGoAllDialog, TuneDialog } from "./UpdatesDialogs";
import { UpdateLineRow } from "./UpdateLineRow";
import { letGo, letGoSource, needsMe, tuneSource, useUpdatesHidden } from "./updatesVerbs";

/**
 * Updates: notifications as a briefing by source, gathered at fixed cuts
 * and let go in one key. Not a list of emails: each line is a source's
 * latest fact, sorted by what it asks of you, and the email is the last
 * resort (`o`).
 */
export function UpdatesRoute() {
  const digest = useDigest();
  const threadIds = useCallback(() => {
    const data = digest.data;
    if (!data) return [];
    const lines = [...data.needs_a_look, ...data.changed, ...data.routine];
    return lines.flatMap((line) => (line.latest_thread_id ? [line.latest_thread_id] : []));
  }, [digest.data]);
  return (
    <PlaceLayout basePath="/updates" label="Updates" threadIds={threadIds}>
      <Briefing status={digest} />
    </PlaceLayout>
  );
}

function Briefing({ status }: { status: ReturnType<typeof useDigest> }) {
  const guide = useModeGuide("updates");
  const phase = useDelayedPending(status.isLoading);
  const data = status.data;
  return (
    <>
      <ModeHeader>
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h1 className="text-[17px] font-semibold tracking-tight text-foreground">Updates</h1>
          <p data-testid="mode-header" className="min-w-0 text-[12.5px] text-muted-foreground">
            {guide.data?.header ??
              data?.header ??
              "Notifications gathered twice a day. Read the digest, then let go."}
          </p>
        </div>
      </ModeHeader>
      {phase !== "ready" ? (
        <ListSkeleton quiet={phase === "quiet"} />
      ) : status.isError ? (
        <Centered
          icon={<RefreshCw className="size-6" />}
          title="Couldn't load Updates"
          body={status.error.message}
          action={
            <Button size="sm" onClick={() => void status.refetch()}>
              Try again
            </Button>
          }
        />
      ) : data ? (
        <Digest digest={data} guide={guide.data} />
      ) : null}
    </>
  );
}

function Digest({ digest, guide }: { digest: UpdatesDigest; guide?: ModeGuide }) {
  const account = useUiPrefs((s) => s.accountScope);
  const nav = useReaderNav();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const hidden = useUpdatesHidden((s) => s.hidden);
  const retire = useRetireCard("updates");
  const [routineOpen, setRoutineOpen] = useState(false);
  const [cursorId, setCursorId] = useState<string | null>(null);
  const [letGoOpen, setLetGoOpen] = useState(false);
  // `e`: the line whose source the open preview is for.
  const [letGoLineOf, setLetGoLineOf] = useState<UpdateLine | null>(null);
  const [tuning, setTuning] = useState<UpdateLine | null>(null);
  const { rows, bySection, quieter } = useMemo(
    () => digestRows(digest, { routineOpen, hidden }),
    [digest, routineOpen, hidden],
  );
  const index = Math.max(
    0,
    rows.findIndex((row) => row.line.id === cursorId),
  );
  const current = rows[index]?.line;
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const hasLines = rows.length > 0;
  const cardShown = Boolean(guide && !guide.card_seen && hasLines);
  const { mutate: retireMutate } = retire;
  const cardSeen = guide?.card_seen ?? true;
  const retireCard = useCallback(() => {
    if (!cardSeen) retireMutate();
  }, [cardSeen, retireMutate]);

  const select = useCallback((line: UpdateLine) => setCursorId(line.id), []);
  const openEmail = useCallback(
    (line: UpdateLine) => {
      if (line.latest_thread_id) nav?.open(line.latest_thread_id);
    },
    [nav],
  );
  const letGoLine = useCallback((line: UpdateLine) => {
    if (!canLetGoSource(line)) return;
    setLetGoLineOf(line);
    setLetGoOpen(true);
  }, []);
  const needs = useCallback((line: UpdateLine) => void needsMe(line), []);
  const tune = useCallback((line: UpdateLine) => {
    if (canTune(line)) setTuning(line);
  }, []);
  const tuneTo = useCallback(
    (line: UpdateLine, setting: SuggestedSetting) => void tuneSource(line, setting),
    [],
  );
  const commitLetGoAll = (selectionToken: string) => {
    // Letting go of the digest is the mode's main verb: the card is spent.
    retireCard();
    if (letGoLineOf) {
      void letGoSource(letGoLineOf, digest.cut.at, selectionToken);
      return;
    }
    void letGo(
      { account, cut: digest.cut.at, selectionToken },
      rows.map((row) => row.line.id),
    );
  };

  const move = (delta: number) => {
    const next = rows[Math.min(rows.length - 1, Math.max(0, index + delta))];
    if (next) setCursorId(next.line.id);
  };
  useShortcutScope("updates", !params.threadId || activePane !== "reader");
  useScopeController("updates", {
    down: () => move(1),
    up: () => move(-1),
    expand: quieter.sources > 0 ? () => setRoutineOpen(true) : undefined,
    letGoAll: digest.let_go_line
      ? () => {
          setLetGoLineOf(null);
          setLetGoOpen(true);
        }
      : undefined,
    letGoSource: () => current && letGoLine(current),
    needsMe: () => current && !current.in_todo && needs(current),
    tune: () => current && tune(current),
    link: () => {
      const link = current && openableLink(current);
      if (link) window.open(link.url, "_blank", "noopener,noreferrer");
    },
    openEmail: () => current && openEmail(current),
    closeCard: cardShown ? retireCard : undefined,
  });

  const section = (name: UpdateSection) => {
    const sectionRows = bySection[name];
    if (sectionRows.length === 0) return null;
    return (
      <section key={name} aria-labelledby={`updates-${name}`} data-testid={`updates-${name}`}>
        <h2
          id={`updates-${name}`}
          className="mx-5 mb-1 mt-4 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground"
        >
          {SECTION_TITLE[name]}
        </h2>
        <ul className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
          {sectionRows.map((row) => (
            <UpdateLineRow
              key={row.line.id}
              line={row.line}
              index={row.index}
              focused={row.index === index}
              onSelect={select}
              onLetGo={letGoLine}
              onNeedsMe={needs}
              onTune={tune}
              onOpenEmail={openEmail}
              onTuneTo={tuneTo}
            />
          ))}
        </ul>
      </section>
    );
  };

  return (
    <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto pb-6">
      <ModeFrame>
        {cardShown && guide ? <ModeCard guide={guide} onClose={retireCard} /> : null}
        <div className="mx-5 mt-3 flex flex-wrap items-center gap-x-4 gap-y-2">
          <p data-testid="updates-cut" className="min-w-0 text-[13px] text-foreground/90">
            {cutLine(digest)}
          </p>
          {digest.let_go_line ? (
            <Button
              size="sm"
              variant="outline"
              className="ml-auto hidden md:inline-flex"
              data-testid="updates-let-go-all"
              onClick={() => {
                setLetGoLineOf(null);
                setLetGoOpen(true);
              }}
            >
              Let go of all <KeyChip className="h-4 px-1">A</KeyChip>
            </Button>
          ) : null}
        </div>
        {digest.headline ? (
          <p data-testid="updates-headline" className="mx-5 mt-1 text-balance text-[15px]">
            {digest.headline}
          </p>
        ) : null}

        {!hasLines ? (
          digest.empty_state ? (
            <div className="mx-5 mt-6 flex items-start gap-3 text-[13px] text-muted-foreground">
              <Bell aria-hidden className="mt-0.5 size-4 shrink-0" />
              <p data-testid="updates-empty">
                {digest.empty_state}
                {digest.source_total > 0
                  ? ` ${plural(digest.source_total, "source")}, ${digest.muted_total} muted.`
                  : null}
              </p>
            </div>
          ) : null
        ) : (
          <div>
            {section("needs_a_look")}
            {section("changed")}
            {section("routine")}
            {quieter.sources > 0 ? (
              <button
                type="button"
                data-testid="updates-quieter"
                aria-expanded={routineOpen}
                onClick={() => setRoutineOpen(true)}
                className="mx-5 mt-1 inline-flex items-center gap-2 text-[12.5px] text-muted-foreground hover:text-foreground"
              >
                + {plural(quieter.sources, "quieter source")} ({quieter.messages})
                <ChevronRight aria-hidden className="size-3" />
              </button>
            ) : null}
          </div>
        )}

        {digest.hidden_line ? (
          <p data-testid="updates-hidden" className="mx-5 mt-4 text-[12.5px] text-muted-foreground">
            {digest.hidden_line}
          </p>
        ) : null}
        {digest.since.message_count > 0 ? (
          <section
            aria-label={digest.since.label}
            data-testid="updates-since"
            className="mx-5 mt-5 border-t border-dashed border-border pt-2 text-[12.5px] text-muted-foreground"
          >
            <p className="flex flex-wrap items-baseline justify-between gap-2">
              <span>{digest.since.label}</span>
              <span className="tabular-nums">{digest.since.message_count} so far</span>
            </p>
            <p className="mt-0.5 text-foreground/80">
              {digest.since.lines
                .map((line) =>
                  line.count > 1 ? `${line.source_name} (${line.count})` : line.source_name,
                )
                .join(", ")}
            </p>
          </section>
        ) : null}
        {digest.expired_line ? (
          <p
            data-testid="updates-expired"
            className="mx-5 mt-4 text-[12.5px] text-muted-foreground"
          >
            {digest.expired_line}
          </p>
        ) : null}
        {guide ? <KeyLine guide={guide} /> : null}

        {digest.let_go_line ? (
          <div className="sticky bottom-0 mt-6 bg-background/95 px-4 pb-3 pt-2 md:hidden">
            <Button
              className="w-full"
              data-testid="updates-let-go-all-mobile"
              onClick={() => {
                setLetGoLineOf(null);
                setLetGoOpen(true);
              }}
            >
              Let go of all {digest.message_count}
            </Button>
          </div>
        ) : null}

        <LetGoAllDialog
          open={letGoOpen}
          onOpenChange={(open) => {
            setLetGoOpen(open);
            if (!open) setLetGoLineOf(null);
          }}
          account={account}
          cut={digest.cut.at}
          source={letGoLineOf}
          onConfirm={commitLetGoAll}
        />
        <TuneDialog
          line={tuning}
          onOpenChange={(open) => !open && setTuning(null)}
          onChoose={(line, setting) => void tuneSource(line, setting)}
        />
      </ModeFrame>
    </div>
  );
}

/** Keys with their verbs at the point of use, from the mode's own table. */
function KeyLine({ guide }: { guide: ModeGuide }) {
  const keys = guide.keys.filter((key) => key.key !== "?" && key.key !== "u");
  return (
    <p className="mx-5 mt-6 hidden flex-wrap items-center gap-x-3 gap-y-1 text-[12px] text-muted-foreground md:flex">
      {keys.map((key) => (
        <span key={key.key} className="inline-flex items-center gap-1">
          <KeyChip>{key.key}</KeyChip> {key.verb}
        </span>
      ))}
    </p>
  );
}
