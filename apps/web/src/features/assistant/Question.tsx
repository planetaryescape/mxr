/*
 * A question the assistant needs answered, on the overlay surface: the
 * question in Heading, the options as rows with a radio or checkbox mark,
 * keys 1 to 6, "Something else" as a row in the same list, and Skip quiet.
 * Once answered it collapses into your quoted reply in the transcript.
 */

import { Circle, CircleCheck, Square, SquareCheck } from "lucide-react";
import { useState, type FormEvent } from "react";

import type { AssistantDecision, AssistantPending, AssistantQuestion } from "./api";
import { onBareKey, PENDING_CARD_ATTRIBUTE } from "./cardKeys";
import { KeyChip } from "@/components/KeyChip";
import { Row } from "@/components/Row";
import { Button } from "@/components/ui/button";
import { isComposingKey } from "@/lib/keys/composition";

/** An answer being written: what is chosen and what is typed. */
interface Draft {
  selected: string[];
  other: string;
}

const MAX_KEYED = 6;
const ROWS = { "--row-height": "52px" } as React.CSSProperties;

export interface Option {
  label: string;
  description?: string | null;
  recommended: boolean;
}

/** The model marks its pick the way Claude Code does, "(Recommended)" at the end of a label; it goes first. */
export function orderedOptions(question: AssistantQuestion): Option[] {
  const options = (question.options ?? []).map((option) => {
    const recommended = /\s*\(recommended\)\s*$/i.test(option.label);
    return {
      label: option.label.replace(/\s*\(recommended\)\s*$/i, ""),
      description: option.description,
      recommended,
    };
  });
  return [...options.filter((o) => o.recommended), ...options.filter((o) => !o.recommended)];
}

export function QuestionCard({
  item,
  busy,
  onDecide,
}: {
  item: Extract<AssistantPending, { kind: "question" }>;
  busy: boolean;
  onDecide: (decision: AssistantDecision) => void;
}) {
  const [answers, setAnswers] = useState<Draft[]>(() =>
    item.questions.map(() => ({ selected: [], other: "" })),
  );
  const complete = answers.every(
    (answer) => answer.selected.length > 0 || answer.other.trim() !== "",
  );
  const update = (index: number, next: Draft) =>
    setAnswers((current) => current.map((answer, i) => (i === index ? next : answer)));
  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    if (!complete || busy) return;
    onDecide({
      call_id: item.call_id,
      decision: "answer",
      answers: answers.map((answer) => ({
        selected: answer.selected,
        other: answer.other.trim() || undefined,
      })),
    });
  };
  const skip = () => {
    if (!busy) onDecide({ call_id: item.call_id, decision: "reject" });
  };
  // Number keys answer the first question not yet answered.
  const active = Math.max(
    0,
    answers.findIndex((answer) => answer.selected.length === 0 && answer.other.trim() === ""),
  );

  return (
    <section
      aria-label="A question for you"
      tabIndex={-1}
      {...{ [PENDING_CARD_ATTRIBUTE]: "", "data-keys-owner": "" }}
      onKeyDown={onBareKey((key) => {
        const question = item.questions[active];
        const answer = answers[active];
        if (!question || !answer) return false;
        const number = Number(key);
        if (Number.isInteger(number) && number >= 1 && number <= MAX_KEYED) {
          const option = orderedOptions(question)[number - 1];
          if (!option) return false;
          update(active, choose(question, answer, option.label));
          return true;
        }
        if (key === "Enter" && complete) {
          submit();
          return true;
        }
        return false;
      })}
      className="border border-rule-soft bg-ink-overlay py-3 outline-none focus-visible:border-signal"
    >
      <form onSubmit={submit} className="space-y-4">
        {item.questions.map((question, index) => (
          <QuestionField
            key={`${item.call_id}:${question.question}`}
            question={question}
            answer={answers[index] ?? { selected: [], other: "" }}
            onChange={(next) => update(index, next)}
            onSubmit={submit}
            keyed={index === active}
          />
        ))}
        <div className="flex items-center gap-1 px-4">
          <button
            type="button"
            disabled={busy}
            onClick={skip}
            className="type-meta h-8 px-1.5 text-text-2 outline-none hover:text-text-1 focus-visible:text-text-1 disabled:opacity-50"
          >
            Skip
          </button>
          <Button type="submit" size="sm" className="ml-auto" disabled={!complete || busy}>
            Answer
            <KeyChip className="pointer-coarse:hidden border-transparent bg-transparent text-inherit shadow-none">
              ↵
            </KeyChip>
          </Button>
        </div>
      </form>
    </section>
  );
}

function choose(question: AssistantQuestion, answer: Draft, label: string): Draft {
  const chosen = answer.selected.includes(label);
  const selected = question.multi_select
    ? chosen
      ? answer.selected.filter((value) => value !== label)
      : [...answer.selected, label]
    : chosen
      ? []
      : [label];
  // A single answer is either an option or typed text, not both.
  return {
    selected,
    other: question.multi_select || selected.length === 0 ? answer.other : "",
  };
}

function QuestionField({
  question,
  answer,
  onChange,
  onSubmit,
  keyed,
}: {
  question: AssistantQuestion;
  answer: Draft;
  onChange: (answer: Draft) => void;
  onSubmit: () => void;
  keyed: boolean;
}) {
  const options = orderedOptions(question);
  const Mark = (chosen: boolean) => {
    const Icon = question.multi_select
      ? chosen
        ? SquareCheck
        : Square
      : chosen
        ? CircleCheck
        : Circle;
    return (
      <Icon
        aria-hidden="true"
        className={chosen ? "size-4 shrink-0 text-signal" : "size-4 shrink-0 text-text-3"}
        strokeWidth={1.75}
      />
    );
  };
  return (
    <fieldset>
      {/* A legend always renders first in its fieldset, so the header
          goes inside it to sit above the question. */}
      <legend className="mb-2 space-y-1 px-4">
        {question.header ? (
          <span className="type-eyebrow block text-text-3">{question.header}</span>
        ) : null}
        <span className="type-heading block text-text-1">{question.question}</span>
      </legend>
      <ul
        role={question.multi_select ? "group" : "radiogroup"}
        aria-label={question.question}
        style={ROWS}
      >
        {options.map((option, index) => {
          const chosen = answer.selected.includes(option.label);
          return (
            <Row
              key={option.label}
              lit={chosen}
              className="mx-0"
              lead={Mark(chosen)}
              primary={
                <>
                  {option.label}
                  {option.recommended ? (
                    <span className="type-meta ml-2 text-signal">recommended</span>
                  ) : null}
                </>
              }
              secondary={option.description ?? undefined}
              trail={
                keyed && index < MAX_KEYED ? (
                  <KeyChip className="h-4 px-1 pointer-coarse:hidden">{index + 1}</KeyChip>
                ) : undefined
              }
              renderMain={(content, className) => (
                <button
                  type="button"
                  role={question.multi_select ? "checkbox" : "radio"}
                  aria-checked={chosen}
                  onClick={() => onChange(choose(question, answer, option.label))}
                  className={className}
                >
                  {content}
                </button>
              )}
            />
          );
        })}
        {question.allow_other ? (
          <Row
            lit={answer.other.trim() !== ""}
            className="mx-0"
            lead={Mark(answer.other.trim() !== "")}
            primary={
              <input
                aria-label="Something else"
                value={answer.other}
                onChange={(event) =>
                  onChange({
                    selected: question.multi_select ? answer.selected : [],
                    other: event.target.value,
                  })
                }
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !isComposingKey(event)) {
                    event.preventDefault();
                    onSubmit();
                  }
                }}
                placeholder="Something else"
                className="type-body w-full bg-transparent text-text-1 outline-none placeholder:text-text-3"
              />
            }
          />
        ) : null}
      </ul>
    </fieldset>
  );
}
