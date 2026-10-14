import { useParams } from "@tanstack/react-router";
import { Bell, ChevronRight, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { KeyChip } from "@/components/KeyChip";
import { KeyLine, type KeyHint } from "@/components/KeyLine";
import { SectionHeading } from "@/components/SectionHeading";
import { Button } from "@/components/ui/button";
import { Centered, ListSkeleton } from "@/features/mailbox/MailViewParts";
import { useReaderNav } from "@/features/mailbox/readerNav";
import { moveCommands, updateMoveSubject } from "@/features/arrivals/moveSubjects";
import { AnchoredHint } from "@/features/hints/AnchoredHint";
import { useActiveHintDismiss, useHint } from "@/features/hints/useHint";
import { useModeGuide, type ModeGuide } from "@/features/modes/api";
import { ModeFrame, ModeHeader } from "@/components/ModeFrame";
import { PlaceLayout } from "@/features/places/PlaceLayout";
import { useDelayedPending } from "@/hooks/useDelayedPending";
import { useShortcutScope } from "@/hooks/useShortcutScope";
import { plural } from "@/lib/format";
import { useScopeController } from "@/lib/keys/controllers";
import { cn } from "@/lib/utils";
import { useMailboxPane } from "@/state/mailboxPaneStore";
import { useUiPrefs } from "@/state/uiPrefsStore";

import { useDigest, type UpdateLine, type UpdateSection, type UpdatesDigest } from "./api";
import {
  cutLine,
  digestRows,
  openableLink,
  routineLine,
  SECTION_TITLE,
  canLetGoSource,
  canPinSource,
  canSweepSource,
  canTune,
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

const DEFAULT_SUBTITLE = "Notifications gathered twice a day. Read the digest, then let go.";

function Briefing({ status }: { status: ReturnType<typeof useDigest> }) {
  const guide = useModeGuide("updates");
  const phase = useDelayedPending(status.isLoading);
  const data = status.data;
  const subtitle = guide.data?.header ?? data?.header ?? DEFAULT_SUBTITLE;
  // Before the digest shows, the header is the title and subtitle alone.
  const plainHeader = <ModeHeader title="Updates" subtitle={subtitle} />;
  if (phase !== "ready") {
    return (
      <>
        {plainHeader}
        <ListSkeleton quiet={phase === "quiet"} />
      </>
    );
  }
  if (status.isError) {
    return (
      <>
        {plainHeader}
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
      </>
    );
  }
  if (data) return <Digest digest={data} guide={guide.data} subtitle={subtitle} />;
  return plainHeader;
}

function Digest({
  digest,
  guide,
  subtitle,
}: {
  digest: UpdatesDigest;
  guide?: ModeGuide;
  subtitle: string;
}) {
  const account = useUiPrefs((s) => s.accountScope);
  const nav = useReaderNav();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activePane = useMailboxPane((s) => s.activePane);
  const hidden = useUpdatesHidden((s) => s.hidden);
  // Routine stays collapsed to its one line until opened, for this visit.
  const [routineOpen, setRoutineOpen] = useState(false);
  const [cursorId, setCursorId] = useState<string | null>(null);
  // Space opens the peek under one line at a time; Escape closes it.
  const [peekId, setPeekId] = useState<string | null>(null);
  const [letGoOpen, setLetGoOpen] = useState(false);
  // `e`: the line whose source the open preview is for.
  const [letGoLineOf, setLetGoLineOf] = useState<UpdateLine | null>(null);
  const [tuning, setTuning] = useState<UpdateLine | null>(null);
  const { rows, bySection, routine } = useMemo(
    () => digestRows(digest, { routineOpen, hidden }),
    [digest, routineOpen, hidden],
  );
  const index = Math.max(
    0,
    rows.findIndex((row) => row.line.id === cursorId),
  );
  const current = rows[index]?.line;
  // A peek belongs to its line: when a refetch, a let go or a tune takes the
  // line away, the peek goes with it, and an undo does not bring it back open.
  useEffect(() => {
    if (peekId !== null && !rows.some((row) => row.line.id === peekId)) setPeekId(null);
  }, [rows, peekId]);
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }, [index]);

  // Routine counts as lines even while it is collapsed.
  const hasLines = rows.length > 0 || routine.sources > 0;
  // One hint under the first suggested to-do, one under the first line,
  // and the digest's let go under its button.
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

  const select = useCallback((line: UpdateLine) => setCursorId(line.id), []);
  const openEmail = useCallback(
    (line: UpdateLine) => {
      if (line.latest_thread_id) nav?.open(line.latest_thread_id);
    },
    [nav],
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
      // With an email open, done moves on: the next source's email opens.
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

  const togglePeek = useCallback(
    (line: UpdateLine) => setPeekId((open) => (open === line.id ? null : line.id)),
    [],
  );
  const move = (delta: number) => {
    const next = rows[Math.min(rows.length - 1, Math.max(0, index + delta))];
    if (next) setCursorId(next.line.id);
  };
  useShortcutScope("updates", !params.threadId || activePane !== "reader");
  useScopeController("updates", {
    down: () => move(1),
    up: () => move(-1),
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
    peek: () => current && togglePeek(current),
    // `K` stays "tune this source"; only the email moves from here.
    moveToMode: moveCommands(() => (current ? updateMoveSubject(current) : null)).moveToMode,
    // Escape closes an open peek first, then a hint.
    closeHint: () => {
      if (peekId !== null) setPeekId(null);
      else closeHint?.();
    },
  });

  const section = (name: UpdateSection) => {
    const sectionRows = bySection[name];
    if (name === "routine") {
      if (routine.sources === 0) return null;
      // Routine is one line until opened; once open it stays open for this visit.
      if (!routineOpen) {
        return (
          <button
            type="button"
            data-testid="updates-routine-toggle"
            aria-expanded={false}
            onClick={() => setRoutineOpen(true)}
            className="mx-5 mb-1 mt-4 flex items-center gap-2 font-mono text-2xs uppercase tracking-[0.12em] text-muted-foreground hover:text-foreground"
          >
            {routineLine(routine)}
            <ChevronRight aria-hidden className="size-3" />
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
              focused={row.index === index}
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
      <ModeHeader
        title="Updates"
        subtitle={subtitle}
        actions={
          digest.let_go_line ? (
            <Button
              size="sm"
              variant="outline"
              className="hidden md:inline-flex"
              data-testid="updates-let-go-all"
              onClick={() => {
                dismissLetGoHint();
                setLetGoLineOf(null);
                setLetGoOpen(true);
              }}
            >
              Let go of all <KeyChip className="h-4 px-1">A</KeyChip>
            </Button>
          ) : null
        }
      />
      <div
        ref={listRef}
        // Below the phone bar, the last row scrolls clear of the sticky Let go of all.
        className={cn("min-h-0 flex-1 overflow-y-auto pb-6", digest.let_go_line && "pb-16 md:pb-6")}
      >
        <ModeFrame>
          <div className="mx-5 mt-3 flex flex-wrap items-center gap-x-4 gap-y-2">
            <p data-testid="updates-cut" className="min-w-0 text-sm text-foreground/90">
              {cutLine(digest)}
            </p>
          </div>
          {letGoHint.hint ? (
            <AnchoredHint className="mx-5" hint={letGoHint.hint} onDismiss={dismissLetGoHint} />
          ) : null}
          {digest.headline ? (
            <p data-testid="updates-headline" className="mx-5 mt-1 text-balance text-md">
              {digest.headline}
            </p>
          ) : null}

          {!hasLines ? (
            digest.empty_state ? (
              <div className="mx-5 mt-6 flex items-start gap-3 text-sm text-muted-foreground">
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
                    line.count > 1 ? `${line.source_name} (${line.count})` : line.source_name,
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
          {guide ? <KeyLine keys={footKeys(guide)} /> : null}

          {digest.let_go_line ? (
            <div className="sticky bottom-0 mt-6 bg-background/95 px-4 pb-3 pt-2 md:hidden">
              <Button
                variant="outline"
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
    </>
  );
}

/** The mode's own key table, without the two keys the foot line leaves out. */
function footKeys(guide: ModeGuide): KeyHint[] {
  return guide.keys
    .filter((key) => key.key !== "?" && key.key !== "u")
    .map((key) => ({ key: key.key, verb: key.verb }));
}
