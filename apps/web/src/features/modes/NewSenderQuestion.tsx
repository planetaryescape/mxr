import { correctSender } from "@/features/places/senderKind";
import { cn } from "@/lib/utils";

import type { ScreenerQuestion } from "./membership";

export function NewSenderQuestion({
  question,
  label,
  inOption = false,
}: {
  question: ScreenerQuestion;
  label: string;
  inOption?: boolean;
}) {
  const Answer = inOption ? "span" : "button";
  return (
    <div
      data-testid="new-sender-question"
      role={inOption ? undefined : "group"}
      aria-label={inOption ? undefined : question.question}
      className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs"
    >
      <span className="text-foreground/85">{question.question}</span>
      {question.choices.map((choice, position) => (
        <Answer
          key={choice.kind}
          {...(inOption ? { "aria-hidden": true } : { type: "button" as const })}
          onClick={(event) => {
            event.stopPropagation();
            void correctSender(
              { accountId: question.account_id, senderEmail: question.sender_email, label },
              choice.kind,
            );
          }}
          className={cn(
            "cursor-pointer underline decoration-border-strong underline-offset-4 hover:decoration-foreground",
            position === 0 ? "text-foreground" : "text-muted-foreground hover:text-foreground",
          )}
        >
          {choice.label}
        </Answer>
      ))}
    </div>
  );
}
