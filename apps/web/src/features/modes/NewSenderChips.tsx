import { Ban, Bell, MessagesSquare, Newspaper, type LucideIcon } from "lucide-react";

import { correctSender } from "@/features/places/senderKind";
import type { SenderKind } from "@/features/places/api";
import { definedShortcut } from "@/lib/actions/definition";
import { cn } from "@/lib/utils";

import type { ScreenerQuestion } from "./membership";
import { optionLetter } from "@/lib/keys/overlayKeys";

const MOVE_KEY = definedShortcut("modes.move");

interface SenderAnswer {
  kind: SenderKind;
  key: string;
  label: string;
  Glyph: LucideIcon;
}

export const NEW_SENDER_ANSWERS: readonly SenderAnswer[] = [
  {
    kind: "people",
    key: optionLetter("new-sender-picker.people"),
    label: "Messages",
    Glyph: MessagesSquare,
  },
  {
    kind: "paper_trail",
    key: optionLetter("new-sender-picker.paper-trail"),
    label: "Updates",
    Glyph: Bell,
  },
  {
    kind: "reading",
    key: optionLetter("new-sender-picker.reading"),
    label: "Reading",
    Glyph: Newspaper,
  },
  {
    kind: "screened_out",
    key: optionLetter("new-sender-picker.screened-out"),
    label: "Block",
    Glyph: Ban,
  },
];

export const NEW_SENDER_NOTE = `New sender: ${MOVE_KEY} then ${NEW_SENDER_ANSWERS.map((c) => `${c.key} ${c.label}`).join(" · ")}`;

export function answerNewSender(
  question: ScreenerQuestion,
  label: string,
  kind: SenderKind,
): Promise<boolean> {
  return correctSender(
    { accountId: question.account_id, senderEmail: question.sender_email, label },
    kind,
  );
}

export function NewSenderChips({
  question,
  label,
  className,
}: {
  question: ScreenerQuestion;
  label: string;
  className?: string;
}) {
  return (
    <div
      data-testid="new-sender-question"
      role="group"
      aria-label={question.question}
      className={cn("flex items-center gap-1.5", className)}
    >
      {NEW_SENDER_ANSWERS.map(({ kind, key, label: name, Glyph }) => (
        <button
          key={kind}
          type="button"
          title={`${name} (${MOVE_KEY} then ${key})`}
          aria-label={name}
          onClick={(event) => {
            event.stopPropagation();
            void answerNewSender(question, label, kind);
          }}
          className="inline-flex h-6 items-center gap-1 rounded-md border border-rule px-1.5 text-text-2 hover:border-text-3 hover:text-text-1 focus-visible:border-signal"
        >
          <Glyph aria-hidden className="size-3.5" strokeWidth={1.75} />
        </button>
      ))}
    </div>
  );
}
