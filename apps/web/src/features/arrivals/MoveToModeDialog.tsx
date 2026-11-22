import { useRef, type KeyboardEvent } from "react";

import { KeyChip } from "@/components/KeyChip";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { NEW_SENDER_ANSWERS, answerNewSender } from "@/features/modes/NewSenderChips";
import type { SenderKind } from "@/features/places/api";
import { cn } from "@/lib/utils";

import { performMove, type MoveSubject } from "./moves";
import { MODE_NAMES, modeForKey, MOVE_KEYS, SENDER_MODES, type ModeId } from "./types";
import { pressedOverlayKey } from "@/lib/keys/overlayKeys";
import { definedKeys } from "@/lib/actions/definition";

const OPTION = "data-move-option";

const SENDER_CHOICES = [
  ["messages", "move-picker.sender-messages", "move-picker-sender.messages"],
  ["updates", "move-picker.sender-updates", "move-picker-sender.updates"],
  ["reading", "move-picker.sender-reading", "move-picker-sender.reading"],
] as const satisfies readonly (readonly [ModeId, string, string])[];

export function pickerChoice(
  key: string,
  sender: boolean,
): { mode: ModeId; sender: boolean } | null {
  if (key.length !== 1) return null;
  const forSender = SENDER_CHOICES.find(([, capital, either]) =>
    definedKeys(sender ? either : capital).includes(key),
  );
  if (forSender) return { mode: forSender[0], sender: true };
  if (sender) return null;
  const mode = modeForKey(key);
  return mode ? { mode, sender: false } : null;
}

export function newSenderChoice(key: string, sender: boolean): SenderKind | null {
  if (key.length !== 1 || (!sender && key === key.toLowerCase())) return null;
  return NEW_SENDER_ANSWERS.find((answer) => answer.key === key.toUpperCase())?.kind ?? null;
}

export function MoveToModeDialog({
  subject,
  sender,
  onClose,
}: {
  subject: MoveSubject;
  sender: boolean;
  onClose: () => void;
}) {
  const contentRef = useRef<HTMLDivElement>(null);
  const newSender = subject.newSender;
  const choose = (mode: ModeId, forSender: boolean) => {
    onClose();
    if (subject.notSure) {
      subject.notSure.answer(mode, forSender);
      return;
    }
    void performMove({ messageId: subject.messageId, mode, sender: forSender });
  };
  const answer = (kind: SenderKind) => {
    if (!newSender) return;
    onClose();
    void answerNewSender(newSender.question, newSender.label, kind);
  };
  const emailModes = sender
    ? newSender
      ? []
      : Object.entries(MOVE_KEYS).filter(([, mode]) => SENDER_MODES.includes(mode))
    : Object.entries(MOVE_KEYS);
  const who = subject.senderLabel ?? newSender?.label ?? "this sender";
  const start = subject.notSure?.mode;
  const here = start ?? subject.here;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const down = pressedOverlayKey(event, "overlay.down");
    if (down || pressedOverlayKey(event, "overlay.up")) {
      event.preventDefault();
      step(down ? 1 : -1);
      return;
    }
    if (newSender) {
      const kind = newSenderChoice(event.key, sender);
      if (kind) {
        event.preventDefault();
        answer(kind);
        return;
      }
      if (sender) return;
    }
    const choice = pickerChoice(event.key, sender);
    if (!choice) return;
    event.preventDefault();
    choose(choice.mode, choice.sender);
  };

  function step(delta: 1 | -1) {
    const options = [...(contentRef.current?.querySelectorAll<HTMLElement>(`[${OPTION}]`) ?? [])];
    if (options.length === 0) return;
    const at = options.indexOf(document.activeElement as HTMLElement);
    const next = at < 0 ? (delta > 0 ? 0 : options.length - 1) : at + delta;
    options[Math.min(options.length - 1, Math.max(0, next))]?.focus();
  }

  const optionClass =
    "flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm outline-none hover:bg-accent focus-visible:bg-accent";
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        ref={contentRef}
        className="max-w-sm"
        data-testid="move-to-mode-dialog"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          const options = contentRef.current?.querySelectorAll<HTMLElement>(`[${OPTION}]`);
          const lit =
            (start && [...(options ?? [])].find((option) => option.dataset.mode === start)) ||
            options?.[0];
          lit?.focus();
        }}
        onKeyDown={onKeyDown}
      >
        <DialogHeader>
          <DialogTitle>
            {sender
              ? `Send everything from ${who} to…`
              : subject.notSure
                ? `Where should "${subject.label}" go?`
                : `Move "${subject.label}" to…`}
          </DialogTitle>
          <DialogDescription>
            {sender
              ? "Their mail goes there from now on."
              : `A letter moves this email; its capital moves everything from ${who}.`}
          </DialogDescription>
        </DialogHeader>
        {emailModes.length > 0 ? (
          <ul className="grid gap-1">
            {emailModes.map(([key, mode]) => (
              <li key={mode}>
                <button
                  type="button"
                  {...{ [OPTION]: "" }}
                  data-mode={mode}
                  data-testid={`move-choice-${mode}`}
                  aria-current={mode === here ? "true" : undefined}
                  onClick={() => choose(mode, sender)}
                  className={optionClass}
                >
                  <span className="flex-1 font-medium">
                    {MODE_NAMES[mode]}
                    {mode === here ? (
                      <span className="ml-2 text-2xs font-normal text-muted-foreground">
                        here now
                      </span>
                    ) : null}
                  </span>
                  <KeyChip>{sender ? key.toUpperCase() : key}</KeyChip>
                </button>
              </li>
            ))}
          </ul>
        ) : null}
        {!sender && !newSender ? (
          <section aria-label={`Everything from ${who}`} className="grid gap-1">
            <h3 className="px-3 text-2xs text-muted-foreground">Everything from {who}</h3>
            <ul className="grid gap-1">
              {Object.entries(MOVE_KEYS)
                .filter(([, mode]) => SENDER_MODES.includes(mode))
                .map(([key, mode]) => (
                  <li key={mode}>
                    <button
                      type="button"
                      {...{ [OPTION]: "" }}
                      data-testid={`move-sender-choice-${mode}`}
                      onClick={() => choose(mode, true)}
                      className={optionClass}
                    >
                      <span className="flex-1 font-medium">{MODE_NAMES[mode]}</span>
                      <KeyChip>{key.toUpperCase()}</KeyChip>
                    </button>
                  </li>
                ))}
            </ul>
          </section>
        ) : null}
        {newSender ? (
          <section aria-label={`New sender: ${who}`} className="grid gap-1">
            <h3 className="px-3 text-2xs text-muted-foreground">
              {sender
                ? "New sender, for their mail from now on"
                : `New sender: everything from ${who}`}
            </h3>
            <ul className="grid gap-1">
              {NEW_SENDER_ANSWERS.map(({ kind, key, label, Glyph }) => (
                <li key={kind}>
                  <button
                    type="button"
                    {...{ [OPTION]: "" }}
                    data-testid={`new-sender-choice-${kind}`}
                    onClick={() => answer(kind)}
                    className={cn(optionClass)}
                  >
                    <Glyph aria-hidden className="size-3.5 text-muted-foreground" />
                    <span className="flex-1 font-medium">{label}</span>
                    <KeyChip>{key}</KeyChip>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
