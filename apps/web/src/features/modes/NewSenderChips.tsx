import { Ban, Bell, MessagesSquare, Newspaper, type LucideIcon } from "lucide-react";

import { correctSender } from "@/features/places/senderKind";
import type { SenderKind } from "@/features/places/api";
import { definedShortcut } from "@/lib/actions/definition";
import { cn } from "@/lib/utils";

import type { ScreenerQuestion } from "./membership";

const MOVE_KEY = definedShortcut("modes.move");

interface SenderAnswer {
  kind: SenderKind;
  /** The capital that picks it in the `X` picker (either case in `P`'s). */
  key: string;
  label: string;
  Glyph: LucideIcon;
}

/**
 * A new sender's four answers, in fixed order so each key always means the
 * same thing (the daemon's own order puts the current mode first). They are
 * answered through the `X` picker, on the `g` letters' capitals, as every
 * picker letters these destinations (blueprint 35); `D` blocks. The glyphs
 * are the modes' own (docs/blueprint/27 "Iconography"); Block is the one
 * answer that isn't a mode.
 */
export const NEW_SENDER_ANSWERS: readonly SenderAnswer[] = [
  { kind: "people", key: "M", label: "Messages", Glyph: MessagesSquare },
  { kind: "paper_trail", key: "U", label: "Updates", Glyph: Bell },
  { kind: "reading", key: "R", label: "Reading", Glyph: Newspaper },
  { kind: "screened_out", key: "D", label: "Block", Glyph: Ban },
];

/** The status bar's wording for the lit new sender (docs/blueprint/27 "Rows"). */
export const NEW_SENDER_NOTE = `New sender: ${MOVE_KEY} then ${NEW_SENDER_ANSWERS.map((c) => `${c.key} ${c.label}`).join(" · ")}`;

/** Answer the question: the sender's kind, remembered for their future mail. Undo puts it back. */
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

/**
 * The lit new sender's choice, in place of the row's verbs: four mode-glyph
 * chips, their names and keys in the tooltips (D129). The keyboard answers
 * through `X` (the capitals) or `P`.
 */
export function NewSenderChips({
  question,
  label,
  className,
}: {
  question: ScreenerQuestion;
  /** How the toast names the sender. */
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
            // The row under it also selects on click; the answer is its own act.
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
