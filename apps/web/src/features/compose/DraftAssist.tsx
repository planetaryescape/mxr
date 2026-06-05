import { ChevronDown, Loader2, Sparkles } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { DraftQualityBadges } from "./DraftQualityBadges";
import { ToneControls } from "./ToneControls";
import type {
  DraftLengthHint,
  DraftRefineKnobs,
  DraftSuggestionResponse,
  VoiceRegister,
} from "./types";

interface DraftAssistProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  purpose: string;
  onPurposeChange: (value: string) => void;
  register: VoiceRegister;
  onRegisterChange: (value: VoiceRegister) => void;
  length: DraftLengthHint;
  onLengthChange: (value: DraftLengthHint) => void;
  /** True once the user has hand-set the tone/length (overriding inference). */
  overridden: boolean;
  onResetTone: () => void;
  /** Daemon's "Matched to {sender} (tone, length)" note, when available. */
  contextNote: string | null;
  onGenerate: () => void;
  generating: boolean;
  refineContext: string;
  onRefineContextChange: (value: string) => void;
  onRefine: (knobs: DraftRefineKnobs) => void;
  refining: boolean;
  canRefine: boolean;
  suggestion: DraftSuggestionResponse | null;
  busy: boolean;
}

export function DraftAssist({
  open,
  onOpenChange,
  purpose,
  onPurposeChange,
  register,
  onRegisterChange,
  length,
  onLengthChange,
  overridden,
  onResetTone,
  contextNote,
  onGenerate,
  generating,
  refineContext,
  onRefineContextChange,
  onRefine,
  refining,
  canRefine,
  suggestion,
  busy,
}: DraftAssistProps) {
  const refineDisabled = busy || refining || !canRefine;
  return (
    <Collapsible
      open={open}
      onOpenChange={onOpenChange}
      className="shrink-0 border-b border-border"
    >
      <div className="mx-auto w-full max-w-[860px] px-5">
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="group flex w-full items-center gap-2 py-2.5 text-left outline-none"
          >
            <Sparkles className="size-4 shrink-0 text-primary" />
            <span className="text-xs font-medium text-foreground">Draft for me</span>
            <span className="hidden min-w-0 truncate text-2xs text-muted-foreground sm:inline">
              Knows how you write to this person — describe it and edit freely.
            </span>
            <DraftQualityBadges suggestion={suggestion} compact />
            <ChevronDown className="ml-auto size-4 shrink-0 text-muted-foreground transition-transform duration-150 group-data-[state=open]:rotate-180" />
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="space-y-3 pb-3">
            <div className="grid gap-3 lg:grid-cols-[1fr_auto] lg:items-end">
              <div>
                <Label htmlFor="compose-ai-purpose">What should this say?</Label>
                <Textarea
                  id="compose-ai-purpose"
                  value={purpose}
                  onChange={(event) => onPurposeChange(event.target.value)}
                  placeholder="Follow up on the deck and ask for feedback by Friday"
                  className="mt-1 min-h-16"
                />
              </div>
              <Button
                type="button"
                onClick={onGenerate}
                disabled={busy || generating}
                className="gap-1.5"
              >
                {generating ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Sparkles className="size-3.5" />
                )}
                Generate
              </Button>
            </div>

            <ToneControls
              contextNote={contextNote}
              register={register}
              onRegisterChange={onRegisterChange}
              length={length}
              onLengthChange={onLengthChange}
              overridden={overridden}
              onResetTone={onResetTone}
              idPrefix="compose-ai"
              idleHint="Tone auto-matched to this recipient"
            />

            <div className="flex flex-wrap items-end gap-2 border-t border-border pt-3">
              <div className="min-w-[220px] flex-1">
                <Label htmlFor="compose-refine-context">Refine context</Label>
                <Input
                  id="compose-refine-context"
                  value={refineContext}
                  onChange={(event) => onRefineContextChange(event.target.value)}
                  placeholder="Optional extra context for refinement"
                  className="mt-1 h-8"
                />
              </div>
              {(
                [
                  { label: "Shorter", knobs: { shorter: true } },
                  { label: "Warmer", knobs: { warmer: true } },
                  { label: "More formal", knobs: { more_formal: true } },
                  { label: "Less emoji", knobs: { less_emoji: true } },
                ] as const
              ).map((option) => (
                <Button
                  key={option.label}
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={refineDisabled}
                  onClick={() => onRefine(option.knobs)}
                >
                  {option.label}
                </Button>
              ))}
            </div>
            {!canRefine ? (
              <p className="text-2xs text-muted-foreground">
                Refine works on saved mxr drafts opened from the drafts list.
              </p>
            ) : null}
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
}
