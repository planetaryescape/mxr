import { Ban, Bell, MessagesSquare, Newspaper, type LucideIcon } from "lucide-react";

import { correctSender } from "@/features/places/senderKind";
import type { SenderKind } from "@/features/places/api";
import { definedShortcut } from "@/lib/actions/definition";
import { cn } from "@/lib/utils";

import type { ScreenerQuestion } from "./membership";

interface SenderChoice {
  kind: SenderKind;
  /** N in `now.sender-N` and `messages.sender-N`, the actions that answer with it. */
  slot: number;
  key: string;
  label: string;
  Glyph: LucideIcon;
}

// One set of chips serves Now and Messages, so Now's keys stand for both;
// a test holds the two modes' keys equal.
function keyFor(slot: number): string {
  return definedShortcut(`now.sender-${slot}`);
}

/**
 * A new sender's four answers, in fixed order so each key always means the
 * same thing (the daemon's own order puts the current mode first). The
 * glyphs are the modes' own (docs/blueprint/27 "Iconography"); Block is the
 * one answer that isn't a mode.
 */
export const SENDER_CHOICES: readonly SenderChoice[] = [
  { kind: "people", slot: 1, key: keyFor(1), label: "Messages", Glyph: MessagesSquare },
  { kind: "paper_trail", slot: 2, key: keyFor(2), label: "Updates", Glyph: Bell },
  { kind: "reading", slot: 3, key: keyFor(3), label: "Reading", Glyph: Newspaper },
  { kind: "screened_out", slot: 4, key: keyFor(4), label: "Block", Glyph: Ban },
];

/** The status bar's wording for the lit new sender (docs/blueprint/27 "Rows"). */
export const NEW_SENDER_NOTE = `New sender: ${SENDER_CHOICES.map((c) => `${c.key} ${c.label}`).join(" · ")}`;

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
 * chips with keys 1 to 4, their names in the tooltips (D129). Keys are bound
 * by the page while this row is lit (`newSenderCommands`).
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
      {SENDER_CHOICES.map(({ kind, key, label: name, Glyph }) => (
        <button
          key={kind}
          type="button"
          title={`${name} (${key})`}
          aria-label={name}
          onClick={(event) => {
            // The row under it also selects on click; the answer is its own act.
            event.stopPropagation();
            void answerNewSender(question, label, kind);
          }}
          className="inline-flex h-6 items-center gap-1 rounded-md border border-rule px-1.5 text-text-2 hover:border-text-3 hover:text-text-1 focus-visible:border-signal"
        >
          <Glyph aria-hidden className="size-3.5" strokeWidth={1.75} />
          <span className="pointer-coarse:hidden">{key}</span>
        </button>
      ))}
    </div>
  );
}

/**
 * Scope-controller commands for keys 1 to 4 while a new sender's row is lit.
 * Empty otherwise, so a scoped binding without a command stays unresolved and
 * the digits keep their global meaning (the retired tab keys say where the
 * tabs went until 0.6.180).
 */
export function newSenderCommands(
  question: ScreenerQuestion | null | undefined,
  label: string,
): Record<string, () => void> {
  if (!question) return {};
  return Object.fromEntries(
    SENDER_CHOICES.map(({ kind, slot }) => [
      `senderChoice${slot}`,
      () => void answerNewSender(question, label, kind),
    ]),
  );
}
