import { useParams } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { LampVia } from "@/components/Row";
import { SectionHeading } from "@/components/SectionHeading";
import { Button } from "@/components/ui/button";
import { PageEmpty, PageError } from "@/components/PageParts";
import { UpdatesSkeleton } from "@/features/modes/ModeSkeletons";
import { FinishingMoment } from "@/features/low-tide/FinishingMoment";
import { useLowTide } from "@/features/low-tide/lowTideMemory";
import { NextPlace } from "@/features/modes/NextPlace";
import { useReaderNav } from "@/features/mailbox/readerNav";
import { moveCommands, updateMoveSubject } from "@/features/arrivals/moveSubjects";
import { requestRowGists, useGistEpoch } from "@/features/gists/rowGists";
import { AnchoredHint } from "@/features/hints/AnchoredHint";
import { useActiveHintDismiss, useHint } from "@/features/hints/useHint";
import { useModeGuide } from "@/features/modes/api";
import { ModeFrame } from "@/components/ModeFrame";
import { PageFrame } from "@/components/PageFrame";
import { PlaceLayout } from "@/features/places/PlaceLayout";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useSearchParam } from "@/hooks/useSearchParam";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useLitReason } from "@/state/litReasonStore";
import { stripKeys } from "@/lib/actions/definition";
import { usePageKeys } from "@/state/pageKeysStore";
import { useUiPrefs } from "@/state/uiPrefsStore";
import { listMotions } from "@/features/modes/listMotions";
import { useMarkRowSeen } from "@/features/modes/rowSeen";

import {
  useDigest,
  useLatestCode,
  type UpdateLine,
  type UpdateSection,
  type UpdatesDigest,
} from "./api";
import { copyCode, lineSender } from "./copyCode";
import { codeSender, LatestCode } from "./LatestCode";
import { displaySourceName } from "./digestView";
import {
  cutLabel,
  cutLine,
  digestRows,
  openableLink,
  routineLine,
  SECTION_TITLE,
  canLetGoSource,
  canPinSource,
  canSweepSource,
  canTune,
  gistThread,
  type SuggestedSetting,
} from "./digestView";
import { LetGoAllDialog, TuneDialog } from "./UpdatesDialogs";
import { UpdateLineRow } from "./UpdateLineRow";
import {
  letGo,
  letGoSource,
  needsMe,
  sweepSource,
  togglePinSource,
  tuneSource,
  useUpdatesHidden,
} from "./updatesVerbs";

const UPDATES_LIST_SEARCH = ["row", "peek"] as const;

export function UpdatesRoute() {
  const digest = useDigest();
  const threadIds = useCallback(() => {
    const data = digest.data;
    if (!data) return [];
    const lines = [...data.needs_a_look, ...data.changed, ...data.routine];
    return lines.flatMap((line) => (line.latest_thread_id ? [line.latest_thread_id] : []));
  }, [digest.data]);
  return (
    <PlaceLayout
      basePath="/updates"
      label="Updates"
      threadIds={threadIds}
      keepSearch={UPDATES_LIST_SEARCH}
    >
      <Briefing status={digest} />
    </PlaceLayout>
  );
}

const GIST_REQUEST_DELAY_MS = 150;

const DEFAULT_SUBTITLE = "Notifications gathered twice a day. Read the digest, then let go.";

function Briefing({ status }: { status: ReturnType<typeof useDigest> }) {
  const guide = useModeGuide("updates");
  const phase = useDelayedPending(status.isLoading);
  const data = status.data;
  const subtitle = guide.data?.header ?? (phase === "ready" ? data?.header : undefined) ?? DEFAULT_SUBTITLE;
  const plainHeader = <PageFrame title="Updates" purpose={subtitle} />;
  if (phase !== "ready") {
    return (
      <>
        {plainHeader}
        <UpdatesSkeleton shown={phase === "skeleton"} />
      </>
    );
  }
  if (status.isError) {
    return (
      <>
        {plainHeader}
        <PageError
          framed
          title="Couldn't load Updates"
          error={status.error}
          onRetry={() => void status.refetch()}
        />
      </>
    );
  }
  if (data) return <Digest digest={data} subtitle={subtitle} />;
  return plainHeader;
}

function Digest({ digest, subtitle }: { digest: UpdatesDigest; subtitle: string }) {
  const latestCode = useLatestCode().data ?? null;
  const account = useUiPrefs((s) => s.accountScope);
  const nav = useReaderNav();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const hidden = useUpdatesHidden((s) => s.hidden);
  const [routineOpen, setRoutineOpen] = useState(false);
  const [cursorId, setCursorId] = useSearchParam("row");
  const [peekId, setPeekId] = useSearchParam("peek");
  const [letGoOpen, setLetGoOpen] = useState(false);
  const via = useRef<LampVia>("key");
  const [letGoLineOf, setLetGoLineOf] = useState<UpdateLine | null>(null);
  const [tuning, setTuning] = useState<UpdateLine | null>(null);
  const { rows, bySection, routine } = useMemo(
    () => digestRows(digest, { routineOpen, hidden }),
    [digest, routineOpen, hidden],
  );
  const gistEpoch = useGistEpoch();
  const gistThreads = useMemo(() => rows.flatMap((row) => gistThread(row.line) ?? []), [rows]);
  useEffect(() => {
    if (gistThreads.length === 0) return;
    const handle = window.setTimeout(() => {
      void requestRowGists(gistThreads, Date.now(), { automated: true });
    }, GIST_REQUEST_DELAY_MS);
    return () => window.clearTimeout(handle);
  }, [gistEpoch, gistThreads]);
  const index = Math.max(
    0,
    rows.findIndex((row) => row.line.id === cursorId),
  );
  const current = rows[index]?.line;
  const folded = useCallback(
    (id: string | null) =>
      id !== null && !routineOpen && digest.routine.some((line) => line.id === id),
    [digest.routine, routineOpen],
  );
  const linkedLines = useRef([cursorId, peekId]);
  useEffect(() => {
    const linked = linkedLines.current;
    linkedLines.current = [];
    if (linked.some(folded)) setRoutineOpen(true);
  }, [folded]);
  useEffect(() => {
    if (peekId !== null && !folded(peekId) && !rows.some((row) => row.line.id === peekId))
      setPeekId(null);
  }, [folded, rows, peekId, setPeekId]);
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const hasLines = rows.length > 0 || routine.sources > 0;
  const finished = useLowTide("updates", false, hasLines, { daily: true });
  const firstSuggestion = rows.find((row) => row.line.todo_suggestion && !row.line.in_todo)?.line;
  const firstLine = rows[0]?.line;
  const alone = rows.length === 1;
  const suggestionHint = useHint("updates", "updates.suggestion", {
    ready: Boolean(firstSuggestion),
    alone,
  });
  const sourceHint = useHint("updates", "updates.source", {
    ready: Boolean(firstLine && canLetGoSource(firstLine)),
    alone,
  });
  const letGoHint = useHint("updates", "updates.let_go", { ready: Boolean(digest.let_go_line) });
  const closeHint = useActiveHintDismiss();
  const hintFor = (line: UpdateLine) => {
    const shown =
      line.id === firstSuggestion?.id && suggestionHint.hint
        ? suggestionHint
        : line.id === firstLine?.id && sourceHint.hint
          ? sourceHint
          : null;
    return shown?.hint ? <AnchoredHint hint={shown.hint} onDismiss={shown.dismiss} /> : null;
  };
  const { dismiss: dismissSuggestionHint } = suggestionHint;
  const { dismiss: dismissSourceHint } = sourceHint;
  const { dismiss: dismissLetGoHint } = letGoHint;

  const select = useCallback(
    (line: UpdateLine) => {
      via.current = "pointer";
      setCursorId(line.id);
    },
    [setCursorId],
  );
  const markRowSeen = useMarkRowSeen("updates");
  const openedThread = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (!params.threadId) {
      openedThread.current = undefined;
      return;
    }
    if (openedThread.current === params.threadId) return;
    const opened = [
      ...digest.needs_a_look,
      ...digest.changed,
      ...digest.routine,
      ...digest.since.lines,
    ].find((line) => line.latest_thread_id === params.threadId);
    if (opened) {
      openedThread.current = params.threadId;
      markRowSeen(opened);
    }
  }, [params.threadId, digest, markRowSeen]);
  const openEmail = useCallback(
    (line: UpdateLine) => {
      if (line.latest_thread_id) {
        openedThread.current = line.latest_thread_id;
        markRowSeen(line);
        nav?.open(line.latest_thread_id);
      }
    },
    [nav, markRowSeen],
  );
  const letGoLine = useCallback(
    (line: UpdateLine) => {
      if (!canLetGoSource(line)) return;
      dismissSourceHint();
      setLetGoLineOf(line);
      setLetGoOpen(true);
    },
    [dismissSourceHint],
  );
  const needs = useCallback(
    (line: UpdateLine) => {
      dismissSuggestionHint();
      void needsMe(line);
    },
    [dismissSuggestionHint],
  );
  const tune = useCallback((line: UpdateLine) => {
    if (canTune(line)) setTuning(line);
  }, []);
  const sweep = useCallback((line: UpdateLine) => {
    if (canSweepSource(line)) sweepSource(line);
  }, []);
  const pin = useCallback((line: UpdateLine) => {
    if (canPinSource(line)) void togglePinSource(line);
  }, []);
  const tuneTo = useCallback(
    (line: UpdateLine, setting: SuggestedSetting) => void tuneSource(line, setting),
    [],
  );
  const commitLetGoAll = (selectionToken: string) => {
    if (letGoLineOf) {
      void letGoSource(letGoLineOf, digest.cut.at, selectionToken);
      if (params.threadId) {
        const at = rows.findIndex((row) => row.line.id === letGoLineOf.id);
        const next = rows
          .slice(at + 1)
          .concat(rows.slice(0, Math.max(at, 0)))
          .find(
            (row) => row.line.source_key !== letGoLineOf.source_key && row.line.latest_thread_id,
          );
        if (next?.line.latest_thread_id) {
          setCursorId(next.line.id);
          nav?.open(next.line.latest_thread_id);
        }
      }
      return;
    }
    void letGo(
      { account, cut: digest.cut.at, selectionToken },
      rows.map((row) => row.line.id),
    );
  };

  const peekRef = useRef(peekId);
  peekRef.current = peekId;
  const togglePeek = useCallback(
    (line: UpdateLine) => setPeekId(peekRef.current === line.id ? null : line.id),
    [setPeekId],
  );
  const move = (delta: number) => {
    const next = rows[Math.min(rows.length - 1, Math.max(0, index + delta))];
    if (!next) return;
    via.current = "key";
    setCursorId(next.line.id);
  };
  useLitReason(current?.why, null, current?.account_id);
  usePageKeys(
    current && (!params.threadId || activePane !== "reader")
      ? stripKeys("updates", current.code || latestCode ? [] : ["updates.copy-code"])
      : [],
  );
  useShortcutScope("updates", !params.threadId || activePane !== "reader");
  useScopeController("updates", {
    down: () => move(1),
    up: () => move(-1),
    ...listMotions(move),
    expand: routine.sources > 0 ? () => setRoutineOpen(true) : undefined,
    letGoAll: digest.let_go_line
      ? () => {
          dismissLetGoHint();
          setLetGoLineOf(null);
          setLetGoOpen(true);
        }
      : undefined,
    letGoSource: () => current && letGoLine(current),
    needsMe: () => current && !current.in_todo && needs(current),
    tune: () => current && tune(current),
    sweep: () => current && sweep(current),
    pin: () => current && pin(current),
    link: () => {
      const link = current && openableLink(current);
      if (link) window.open(link.url, "_blank", "noopener,noreferrer");
    },
    openEmail: () => current && openEmail(current),
    copyCode: current?.code
      ? () =>
          current.code &&
          copyCode(
            current.code,
            lineSender(displaySourceName(current.source_name), current.sender_email),
          )
      : latestCode
        ? () => copyCode(latestCode.code, codeSender(latestCode))
        : undefined,
    peek: () => current && togglePeek(current),
    moveToMode: moveCommands(() => (current ? updateMoveSubject(current) : null)).moveToMode,
    closeHint: () => {
      if (peekId !== null) setPeekId(null);
      else closeHint?.();
    },
  });

  const section = (name: UpdateSection) => {
    const sectionRows = bySection[name];
    if (name === "routine") {
      if (routine.sources === 0) return null;
      if (!routineOpen) {
        return (
          <button
            type="button"
            data-testid="updates-routine-toggle"
            aria-expanded={false}
            onClick={() => setRoutineOpen(true)}
            className="type-eyebrow mx-5 mb-1 mt-4 flex items-center gap-2 text-text-3 hover:text-text-1 focus-visible:text-text-1"
          >
            {routineLine(routine)}
            <ChevronRight aria-hidden className="size-3.5" />
          </button>
        );
      }
    }
    if (sectionRows.length === 0) return null;
    return (
      <section key={name} aria-labelledby={`updates-${name}`} data-testid={`updates-${name}`}>
        {name === "routine" ? (
          <SectionHeading id={`updates-${name}`} className="mx-5 mb-1 mt-4">
            {routineLine(routine)}
          </SectionHeading>
        ) : (
          <SectionHeading
            id={`updates-${name}`}
            className="mx-5 mb-1 mt-4"
            count={sectionRows.length}
          >
            {SECTION_TITLE[name]}
          </SectionHeading>
        )}
        <ul className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
          {sectionRows.map((row) => (
            <UpdateLineRow
              key={row.line.id}
              line={row.line}
              index={row.index}
              lit={row.index === index}
              via={via.current}
              peeked={peekId === row.line.id}
              onSelect={select}
              onLetGo={letGoLine}
              onNeedsMe={needs}
              onTune={tune}
              onOpenEmail={openEmail}
              onTuneTo={tuneTo}
              onSweep={sweep}
              onPin={pin}
              onTogglePeek={togglePeek}
              hint={hintFor(row.line)}
            />
          ))}
        </ul>
      </section>
    );
  };

  return (
    <>
      <PageFrame
        title="Updates"
        purpose={subtitle}
        freshness={cutLabel(digest)}
        primary={
          digest.let_go_line ? (
            <Button
              size="sm"
              data-testid="updates-let-go-all"
              title="Let go of all (A)"
              onClick={() => {
                dismissLetGoHint();
                setLetGoLineOf(null);
                setLetGoOpen(true);
              }}
            >
              Let go of all
            </Button>
          ) : null
        }
      />
      <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto pb-6">
        <ModeFrame>
          <p data-testid="updates-cut" className="type-meta mx-5 mt-3 text-text-2">
            {cutLine(digest)}
          </p>
          {latestCode ? <LatestCode code={latestCode} /> : null}
          {letGoHint.hint ? (
            <AnchoredHint className="mx-5" hint={letGoHint.hint} onDismiss={dismissLetGoHint} />
          ) : null}

          {!hasLines ? (
            digest.empty_state ? (
              <div className="mx-5 mt-6">
                <PageEmpty
                  glyph={finished ? undefined : "updates"}
                  moment={finished ? <FinishingMoment /> : undefined}
                  title={digest.empty_state}
                  body={
                    <>
                      {digest.source_total > 0
                        ? `${plural(digest.source_total, "source")}, ${digest.muted_total} muted.`
                        : null}
                      <NextPlace current="updates" />
                    </>
                  }
                />
              </div>
            ) : null
          ) : (
            <div>
              {section("needs_a_look")}
              {section("changed")}
              {section("routine")}
            </div>
          )}

          {digest.hidden_line ? (
            <p data-testid="updates-hidden" className="mx-5 mt-4 text-xs text-muted-foreground">
              {digest.hidden_line}
            </p>
          ) : null}
          {digest.since.message_count > 0 ? (
            <section
              aria-label={digest.since.label}
              data-testid="updates-since"
              className="mx-5 mt-5 border-t border-dashed border-border pt-2 text-xs text-muted-foreground"
            >
              <p className="flex flex-wrap items-baseline justify-between gap-2">
                <span>{digest.since.label}</span>
                <span className="tabular-nums">{digest.since.message_count} so far</span>
              </p>
              <p className="mt-0.5 text-foreground/80">
                {digest.since.lines
                  .map((line) =>
                    line.count > 1
                      ? `${displaySourceName(line.source_name)} (${line.count})`
                      : displaySourceName(line.source_name),
                  )
                  .join(", ")}
              </p>
            </section>
          ) : null}
          {digest.expired_line ? (
            <p data-testid="updates-expired" className="mx-5 mt-4 text-xs text-muted-foreground">
              {digest.expired_line}
            </p>
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
    </>
  );
}
