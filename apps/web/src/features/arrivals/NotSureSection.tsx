import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { toast } from "sonner";

import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { replyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { markModeDone } from "@/features/modes/modeDone";
import { definedShortcut } from "@/lib/actions/definition";

import { openMovePicker, performMove } from "./moves";
import { SENDER_MODES, type ModeId, type NotSure } from "./types";
import { useTrustHint } from "./useTrustHint";
import { optionLetter, pressedOverlayKey } from "@/lib/keys/overlayKeys";

interface PendingAsk {
  question: NotSure;
  mode: ModeId;
}

export function NotSureSection({
  questions,
  heading,
  hint,
}: {
  questions: NotSure[];
  heading?: string;
  hint?: string;
}) {
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

  const answer = (question: NotSure, mode: ModeId, sender = false) => {
    firstHint.dismiss();
    setAnswered((current) => new Set(current).add(question.message_id));
    void performMove(
      { messageId: question.message_id, mode, sender, source: "not_sure" },
      { askInToast: false },
    ).then((outcome) => {
      if (!outcome) {
        setAnswered((current) => {
          const next = new Set(current);
          next.delete(question.message_id);
          return next;
        });
        return;
      }
      settled.current.add(question.message_id);
      setAnswered((current) => new Set(current));
      const moved = outcome.correction_id != null && outcome.from !== outcome.to;
      if (moved && !sender && SENDER_MODES.includes(mode)) {
        setAsk({ question, mode });
      }
    });
  };

  if (shown.length === 0 && !ask) return null;
  return (
    <section aria-labelledby="not-sure-heading" data-testid="not-sure" className="mt-2">
      {shown.length > 0 && heading ? (
        <h2 id="not-sure-heading" className="text-xs text-foreground/90">
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
                className="mt-1 flex items-start gap-2 text-xs text-muted-foreground"
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

const MOVE_KEY = definedShortcut("modes.move");
const NOW_KEYS: ReadonlyMap<string, (question: NotSure) => void> = new Map([
  [definedShortcut("now.done"), doneWithEmail],
  [
    definedShortcut("now.reply"),
    (question: NotSure) =>
      useComposeUi.getState().openCompose(replyIntent(question.message_id, "single"), "overlay"),
  ],
]);

function doneWithEmail(question: NotSure) {
  if (question.mode === "archive") {
    toast.info("Archive has no done: records stay filed.");
    return;
  }
  void markModeDone(question.mode, [question.thread_id]);
}

function NotSureRow({
  question,
  onAnswer,
}: {
  question: NotSure;
  onAnswer: (question: NotSure, mode: ModeId, sender: boolean) => void;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === MOVE_KEY) {
      event.preventDefault();
      openMovePicker({
        messageId: question.message_id,
        label: question.subject || "(no subject)",
        senderLabel: question.sender_name || question.sender_email,
        notSure: {
          mode: question.mode,
          answer: (mode, sender) => onAnswer(question, mode, sender),
        },
      });
      return;
    }
    const nowVerb = event.shiftKey ? undefined : NOW_KEYS.get(event.key);
    if (nowVerb) {
      event.preventDefault();
      nowVerb(question);
    }
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
      <p className="text-sm text-foreground">{question.line}</p>
      <div className="mt-1 flex flex-wrap gap-1">
        {question.choices.map((choice) => (
          <Button
            key={choice.mode}
            size="sm"
            variant="ghost"
            data-testid={`not-sure-${choice.mode}`}
            aria-current={choice.mode === question.mode ? "true" : undefined}
            onClick={() => onAnswer(question, choice.mode, false)}
            className={cn(
              "h-7 gap-1.5 px-2 text-xs",
              choice.mode === question.mode && "text-muted-foreground",
            )}
          >
            {choice.label}
          </Button>
        ))}
      </div>
    </div>
  );
}

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
        if (pressedOverlayKey(event, "sender-ask.yes")) {
          event.preventDefault();
          yes();
        } else if (pressedOverlayKey(event, "sender-ask.no")) {
          event.preventDefault();
          onDone();
        }
      }}
      className="mt-1 flex flex-wrap items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span>Always for {who}?</span>
      <Button size="sm" variant="ghost" className="h-7 gap-1.5 px-2" onClick={yes}>
        <KeyChip>{optionLetter("sender-ask.yes")}</KeyChip> Yes
      </Button>
      <Button size="sm" variant="ghost" className="h-7 gap-1.5 px-2" onClick={onDone}>
        <KeyChip>{optionLetter("sender-ask.no")}</KeyChip> No
      </Button>
    </div>
  );
}
