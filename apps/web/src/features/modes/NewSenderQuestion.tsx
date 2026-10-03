import { correctSender } from "@/features/places/senderKind";

import type { ScreenerQuestion } from "./membership";

/**
 * A first-time person's one question, on their row where their mail landed
 * (D117): "New sender. Keep in Messages? [Messages] [Updates] [Reading]
 * [Block]". Answering is the sender's kind, remembered for their future
 * mail; Undo puts it back.
 */
export function NewSenderQuestion({
  question,
  label,
  inOption = false,
}: {
  question: ScreenerQuestion;
  /** How the toast names the sender. */
  label: string;
  /**
   * Inside a listbox option, which can't hold buttons: the answers are
   * pointer targets, and the keyboard answers with `K` (move sender).
   */
  inOption?: boolean;
}) {
  const Answer = inOption ? "span" : "button";
  return (
    <div
      data-testid="new-sender-question"
      role={inOption ? undefined : "group"}
      aria-label={inOption ? undefined : question.question}
      className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px]"
    >
      <span className="text-foreground/85">{question.question}</span>
      {question.choices.map((choice, position) => (
        <Answer
          key={choice.kind}
          {...(inOption ? { "aria-hidden": true } : { type: "button" as const })}
          onClick={(event) => {
            // The row under it opens on click; the answer is its own act.
            event.stopPropagation();
            void correctSender(
              { accountId: question.account_id, senderEmail: question.sender_email, label },
              choice.kind,
            );
          }}
          className={
            position === 0
              ? "cursor-pointer rounded border border-primary/60 px-1.5 py-0.5 text-foreground hover:bg-accent"
              : "cursor-pointer rounded border border-border px-1.5 py-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          }
        >
          {choice.label}
        </Answer>
      ))}
    </div>
  );
}
