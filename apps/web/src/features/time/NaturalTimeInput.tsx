/*
 * A time field that reads words ("fri 3", "tomorrow 9am", "in 2d") with the
 * daemon's parser and shows, before anything is saved, the exact local time
 * it resolved to. The part of the text that was understood is highlighted
 * in place, assumed parts of the answer are muted, and an ambiguous phrase
 * offers its readings as choices. The caller stores `choice.at` unchanged.
 */

import { useLayoutEffect, useRef, type KeyboardEvent, type Ref } from "react";

import { KeyChip } from "@/components/KeyChip";
import { cn } from "@/lib/utils";

import { describeChoice, type TimeChoice } from "./api";
import { highlightSegments } from "./highlight";
import type { NaturalTimeState } from "./useNaturalTime";

interface NaturalTimeInputProps {
  id: string;
  state: NaturalTimeState;
  /** Called with the reading to store when the user presses Enter. Return
   * the submission's promise so a second Enter waits for it to finish. */
  onCommit: (choice: TimeChoice) => void | Promise<void>;
  placeholder?: string;
  /** Shown before any typing, e.g. 'Try "fri 3", "tomorrow 9am" or "in 2d".' */
  hint?: string;
  autoFocus?: boolean;
  disabled?: boolean;
  inputRef?: Ref<HTMLInputElement>;
  className?: string;
}

// Shared by the input and its highlight layer so the two line up exactly.
const FIELD_TEXT = "px-2 py-1 text-sm leading-5";

export function NaturalTimeInput({
  id,
  state,
  onCommit,
  placeholder,
  hint = 'Try "fri 3", "tomorrow 9am", "next week" or "in 2d".',
  autoFocus,
  disabled,
  inputRef,
  className,
}: NaturalTimeInputProps) {
  const mirrorRef = useRef<HTMLDivElement>(null);
  const fieldRef = useRef<HTMLInputElement | null>(null);
  const { value, resolution, last, failure, pending, choiceIndex, selected } = state;
  const choices = resolution?.choices ?? [];
  const ambiguous = choices.length > 1;
  // While the next answer loads, keep showing the last one (dimmed) rather
  // than blanking the preview on every keystroke.
  const display = value.trim() ? last : null;
  const segments = highlightSegments(
    value,
    display?.resolution?.spans ?? display?.error?.understood ?? [],
  );
  const shownChoice = selected ?? display?.resolution?.choices[0] ?? null;

  // Keep the highlight layer scrolled with the field when text overflows.
  useLayoutEffect(syncScroll);

  function syncScroll() {
    if (mirrorRef.current && fieldRef.current) {
      mirrorRef.current.scrollLeft = fieldRef.current.scrollLeft;
    }
  }

  function commit() {
    void state.commit(onCommit);
  }

  function moveChoice(delta: number): number {
    const next = (choiceIndex + delta + choices.length) % choices.length;
    state.setChoiceIndex(next);
    return next;
  }

  function onFieldKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    // A held Enter repeats; only a deliberate press may commit, so holding
    // it can't reveal an ambiguous phrase's choices and then take one.
    if (event.key === "Enter" && event.repeat) {
      event.preventDefault();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      commit();
    } else if (ambiguous && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      event.preventDefault();
      moveChoice(event.key === "ArrowDown" ? 1 : -1);
    }
  }

  function onChoicesKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
    const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
    if (forward || back) {
      event.preventDefault();
      focusChoice(event.currentTarget, moveChoice(forward ? 1 : -1));
    } else if (/^[1-9]$/.test(event.key) && Number(event.key) <= choices.length) {
      // Digits pick a reading here; the surrounding dialog uses them for
      // presets, so keep the key from reaching it.
      event.preventDefault();
      event.stopPropagation();
      state.setChoiceIndex(Number(event.key) - 1);
      focusChoice(event.currentTarget, Number(event.key) - 1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      if (!event.repeat) commit();
    }
  }

  const describedBy = `${id}-resolution`;
  return (
    <div className={cn("grid gap-1.5", className)}>
      <div className="relative rounded-md border border-border bg-input focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-1 ring-offset-background">
        <div
          ref={mirrorRef}
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-0 overflow-hidden whitespace-pre text-transparent",
            FIELD_TEXT,
          )}
        >
          {segments.map((segment) =>
            segment.understood ? (
              <mark
                key={segment.start}
                data-understood=""
                className="rounded-sm bg-primary/20 text-transparent shadow-[0_0_0_1px] shadow-primary/30"
              >
                {segment.text}
              </mark>
            ) : (
              <span key={segment.start}>{segment.text}</span>
            ),
          )}
        </div>
        <input
          id={id}
          ref={(node) => {
            fieldRef.current = node;
            if (typeof inputRef === "function") inputRef(node);
            else if (inputRef) inputRef.current = node;
          }}
          value={value}
          onChange={(event) => state.setValue(event.target.value)}
          onKeyDown={onFieldKeyDown}
          onScroll={syncScroll}
          onSelect={syncScroll}
          placeholder={placeholder}
          autoFocus={autoFocus}
          disabled={disabled}
          autoComplete="off"
          spellCheck={false}
          aria-describedby={describedBy}
          aria-invalid={state.error ? true : undefined}
          className={cn(
            "relative block w-full rounded-md bg-transparent outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50",
            FIELD_TEXT,
          )}
        />
      </div>

      <p
        id={describedBy}
        role="status"
        aria-live="polite"
        className={cn(
          "min-h-5 text-xs tabular-nums",
          pending && display ? "opacity-60" : undefined,
        )}
      >
        <ResolutionLine
          choice={shownChoice}
          message={
            failure
              ? `Couldn't check that time: ${failure.message}`
              : (display?.error?.message ?? (value.trim() ? null : hint))
          }
        />
      </p>

      {ambiguous ? (
        <div
          role="radiogroup"
          aria-label="Which time did you mean?"
          className="flex flex-wrap items-center gap-1.5"
          onKeyDown={onChoicesKeyDown}
        >
          {choices.map((choice, index) => {
            const checked = index === choiceIndex;
            return (
              <button
                key={choice.at}
                type="button"
                role="radio"
                aria-checked={checked}
                aria-label={describeChoice(choice)}
                tabIndex={checked ? 0 : -1}
                data-choice-index={index}
                onClick={() => state.setChoiceIndex(index)}
                className={cn(
                  "inline-flex h-7 items-center gap-1.5 rounded-md border px-2 font-mono text-xs tabular-nums",
                  checked
                    ? "border-primary bg-primary-muted text-foreground"
                    : "border-border text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                {choice.label}
                <KeyChip className="h-4 px-1">{index + 1}</KeyChip>
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

function focusChoice(group: HTMLElement, index: number) {
  group.querySelector<HTMLElement>(`[data-choice-index="${index}"]`)?.focus();
}

/** "Friday 3 October, 15:00 · in 6 days", with assumed parts muted. */
function ResolutionLine({
  choice,
  message,
}: {
  choice: TimeChoice | null;
  message: string | null;
}) {
  if (message) return <span className="text-muted-foreground">{message}</span>;
  if (!choice) return null;
  const dateAssumed = choice.implied.includes("date");
  const timeAssumed = choice.implied.includes("time") || choice.implied.includes("meridiem");
  return (
    <>
      <span
        data-assumed={dateAssumed || undefined}
        className={dateAssumed ? "text-muted-foreground" : "font-medium text-foreground"}
      >
        {choice.date_label}
      </span>
      <span className="text-muted-foreground">, </span>
      <span
        data-assumed={timeAssumed || undefined}
        className={timeAssumed ? "text-muted-foreground" : "font-medium text-foreground"}
      >
        {choice.time_label}
      </span>
      <span className="text-muted-foreground"> · {choice.relative_label}</span>
      {choice.note ? <span className="block text-muted-foreground">{choice.note}</span> : null}
    </>
  );
}
