import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { performMove } from "./moves";
import { modeForKey, SENDER_MODES, type ModeId, type NotSure } from "./types";
import { useTrustHint } from "./useTrustHint";

/** An answer that moved mail somewhere a sender's mail can go: ask once. */
interface PendingAsk {
  question: NotSure;
  mode: ModeId;
}

/**
 * "2 emails I wasn't sure about. Where should these go?" At most three a
 * day, chosen by the daemon: emails two of its rules disagreed about. One
 * key answers (the mode's `g` letter); keeping it where it is answers too.
 * After a move to Messages, Updates or Reading, one question: always for
 * this sender?
 */
export function NotSureSection({
  questions,
  heading,
  hint,
}: {
  questions: NotSure[];
  heading?: string;
  hint?: string;
}) {
  // Hidden while their answer is on its way. Once the daemon has stopped
  // listing a question that was answered, it is forgotten, so an Undo that
  // brings the question back shows it again.
  const [answered, setAnswered] = useState<ReadonlySet<string>>(new Set());
  const settled = useRef(new Set<string>());
  useEffect(() => {
    const listed = new Set(questions.map((question) => question.message_id));
    const gone = [...answered].filter((id) => settled.current.has(id) && !listed.has(id));
    if (gone.length === 0) return;
    for (const id of gone) settled.current.delete(id);
    setAnswered((current) => new Set([...current].filter((id) => !gone.includes(id))));
  }, [questions, answered]);
  const [ask, setAsk] = useState<PendingAsk | null>(null);
  const shown = questions.filter((question) => !answered.has(question.message_id));
  const firstHint = useTrustHint("now.not_sure", hint, shown.length > 0);

  const answer = (question: NotSure, mode: ModeId) => {
    firstHint.dismiss();
    setAnswered((current) => new Set(current).add(question.message_id));
    void performMove(
      { messageId: question.message_id, mode, source: "not_sure" },
      { askInToast: false },
    ).then((outcome) => {
      // A move that failed (the toast says why) answered nothing: the
      // question comes back.
      if (!outcome) {
        setAnswered((current) => {
          const next = new Set(current);
          next.delete(question.message_id);
          return next;
        });
        return;
      }
      // The answer is in and the lists have refreshed: from here the
      // daemon's own list says whether the question is still open.
      settled.current.add(question.message_id);
      setAnswered((current) => new Set(current));
      // Keeping it where it was moved nothing, so there is nothing to repeat.
      const moved = outcome.correction_id != null && outcome.from !== outcome.to;
      if (moved && SENDER_MODES.includes(mode)) {
        setAsk({ question, mode });
      }
    });
  };

  if (shown.length === 0 && !ask) return null;
  return (
    <section aria-labelledby="not-sure-heading" data-testid="not-sure" className="mt-2">
      {shown.length > 0 && heading ? (
        <h2 id="not-sure-heading" className="text-[12.5px] text-foreground/90">
          {heading}
        </h2>
      ) : null}
      <ul className="mt-1 grid gap-1">
        {shown.map((question, at) => (
          <li key={question.message_id}>
            <NotSureRow question={question} onAnswer={answer} />
            {at === 0 && firstHint.text ? (
              <p
                role="note"
                data-testid="not-sure-hint"
                className="mt-1 flex items-start gap-2 text-[12px] text-muted-foreground"
                onKeyDown={(event) => event.key === "Escape" && firstHint.dismiss()}
              >
                <span className="flex-1">{firstHint.text}</span>
                <button
                  type="button"
                  onClick={firstHint.dismiss}
                  className="underline underline-offset-2 hover:text-foreground"
                >
                  Got it
                </button>
              </p>
            ) : null}
          </li>
        ))}
      </ul>
      {ask ? <SenderAsk ask={ask} onDone={() => setAsk(null)} /> : null}
    </section>
  );
}

function NotSureRow({
  question,
  onAnswer,
}: {
  question: NotSure;
  onAnswer: (question: NotSure, mode: ModeId) => void;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;
    const mode = modeForKey(event.key);
    if (!mode) return;
    event.preventDefault();
    onAnswer(question, mode);
  };
  return (
    <div
      role="group"
      aria-label={question.line}
      tabIndex={0}
      data-keys-owner
      data-testid="not-sure-row"
      onKeyDown={onKeyDown}
      className="rounded-md px-2 py-1.5 outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <p className="text-[13px] text-foreground">{question.line}</p>
      <div className="mt-1 flex flex-wrap gap-1">
        {question.choices.map((choice) => (
          <Button
            key={choice.mode}
            size="sm"
            variant="ghost"
            data-testid={`not-sure-${choice.mode}`}
            aria-current={choice.mode === question.mode ? "true" : undefined}
            onClick={() => onAnswer(question, choice.mode)}
            className={cn(
              "h-7 gap-1.5 px-2 text-[12px]",
              choice.mode === question.mode && "text-muted-foreground",
            )}
          >
            <KeyChip>{choice.key}</KeyChip>
            {choice.label}
          </Button>
        ))}
      </div>
    </div>
  );
}

/** "Always for this sender?", once per answer: y or n. */
function SenderAsk({ ask, onDone }: { ask: PendingAsk; onDone: () => void }) {
  const who = ask.question.sender_name || ask.question.sender_email;
  const yes = () => {
    onDone();
    void performMove({ messageId: ask.question.message_id, mode: ask.mode, sender: true });
  };
  return (
    <div
      role="group"
      aria-label={`Always for ${who}?`}
      tabIndex={-1}
      data-keys-owner
      data-testid="not-sure-sender-ask"
      ref={(node) => node?.focus()}
      onKeyDown={(event) => {
        if (event.key === "y") {
          event.preventDefault();
          yes();
        } else if (event.key === "n" || event.key === "Escape") {
          event.preventDefault();
          onDone();
        }
      }}
      className="mt-1 flex flex-wrap items-center gap-2 rounded-md px-2 py-1.5 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span>Always for {who}?</span>
      <Button size="sm" variant="ghost" className="h-7 gap-1.5 px-2" onClick={yes}>
        <KeyChip>y</KeyChip> Yes
      </Button>
      <Button size="sm" variant="ghost" className="h-7 gap-1.5 px-2" onClick={onDone}>
        <KeyChip>n</KeyChip> No
      </Button>
    </div>
  );
}
