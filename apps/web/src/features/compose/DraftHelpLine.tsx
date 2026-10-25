/*
 * "Draft for me" as one line above the page: an instruction field whose
 * placeholder says what will happen, four adjust chips and a quiet Polish.
 * Closed, it is a single text button. It sends nothing until submitted.
 */

import { PenLine } from "lucide-react";
import { useEffect, useRef } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { isComposingKey } from "@/lib/keys/composition";
import { cn } from "@/lib/utils";
import type { ComposeAssistAdjust } from "./assist/types";
import type { components } from "@/api/generated";

type ModelChoice = components["schemas"]["ModelChoiceData"];

const CHIPS: { key: keyof ComposeAssistAdjust; label: string }[] = [
  { key: "shorter", label: "Shorter" },
  { key: "warmer", label: "Warmer" },
  { key: "more_formal", label: "More formal" },
  { key: "proofread", label: "Proofread" },
];

interface DraftHelpLineProps {
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
  instruction: string;
  onInstructionChange: (value: string) => void;
  placeholder: string;
  adjust: ComposeAssistAdjust;
  onToggleAdjust: (key: keyof ComposeAssistAdjust) => void;
  onSubmit: () => void;
  onPolish: () => void;
  generating: boolean;
  /** Changes each time the line should take focus (⌘J, opening from a reader). */
  focusNonce: number;
  disabled: boolean;
  model?: string;
  defaultModel: string;
  modelChoices: ModelChoice[];
  onModelChange: (model: string | undefined) => void;
}

export function DraftHelpLine({
  open,
  onOpen,
  onClose,
  instruction,
  onInstructionChange,
  placeholder,
  adjust,
  onToggleAdjust,
  onSubmit,
  onPolish,
  generating,
  focusNonce,
  disabled,
  model,
  defaultModel,
  modelChoices,
  onModelChange,
}: DraftHelpLineProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (open && focusNonce > 0) inputRef.current?.focus();
  }, [open, focusNonce]);

  if (!open) {
    return (
      <div className="shrink-0 border-b border-border">
        <div className="mx-auto flex w-full max-w-[860px] px-5 py-1.5">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="gap-1.5 text-muted-foreground"
            onClick={onOpen}
            aria-keyshortcuts="Meta+J"
            title="Draft for me (⌘J)"
          >
            <PenLine className="size-3.5" />
            Draft for me
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="shrink-0 border-b border-border" role="group" aria-label="Draft for me">
      <div className="mx-auto flex w-full max-w-[860px] flex-wrap items-center gap-2 px-5 py-1.5">
        <Input
          ref={inputRef}
          aria-label="Draft instruction"
          value={instruction}
          placeholder={placeholder}
          disabled={disabled}
          onChange={(event) => onInstructionChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !isComposingKey(event)) {
              event.preventDefault();
              if (!generating && !disabled) onSubmit();
            } else if (event.key === "Escape") {
              event.preventDefault();
              onClose();
            }
          }}
          className="h-8 min-w-[220px] flex-1 bg-input text-xs"
        />
        <label className="sr-only" htmlFor="compose-draft-model">
          Drafting model
        </label>
        <select
          id="compose-draft-model"
          aria-label="Drafting model"
          value={model ?? "__default"}
          disabled={disabled}
          onChange={(event) =>
            onModelChange(event.target.value === "__default" ? undefined : event.target.value)
          }
          className="h-8 max-w-48 border border-border bg-background px-2 text-xs text-muted-foreground"
          title="For this draft only. The default is in Settings → Compose."
        >
          <option value="__default">
            {defaultModel ? `${defaultModel} · default` : "Default model"}
          </option>
          {modelChoices.map((choice) => {
            return (
              <option key={choice.id} value={choice.id} disabled={!choice.available}>
                {choice.label}
                {choice.cloud ? " · cloud" : " · this Mac"} · {choice.sees}
                {choice.id === defaultModel ? " · default" : ""}
                {choice.available ? "" : ` · ${choice.unavailable_reason ?? "unavailable"}`}
              </option>
            );
          })}
        </select>
        <div className="flex flex-wrap items-center gap-1.5">
          {CHIPS.map((chip) => {
            const pressed = adjust[chip.key];
            return (
              <Button
                key={chip.key}
                type="button"
                variant="outline"
                size="sm"
                aria-pressed={pressed}
                className={cn("h-7 px-2 text-2xs", pressed && "border-signal text-signal")}
                onClick={() => onToggleAdjust(chip.key)}
              >
                {chip.label}
              </Button>
            );
          })}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-2xs text-text-3"
            onClick={onPolish}
            disabled={generating || disabled}
          >
            Polish
          </Button>
        </div>
        {generating ? (
          <span role="status" className="text-2xs text-text-3">
            Writing…
          </span>
        ) : null}
      </div>
    </div>
  );
}
