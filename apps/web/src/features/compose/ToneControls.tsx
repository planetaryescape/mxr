/*
 * Shared inferred-tone control: a chip showing what the daemon matched
 * ("Matched to alice@x (casual, short)") plus an "Adjust" disclosure with
 * Register/Length dials and a "Reset to auto". Used by both the compose
 * "Draft for me" panel and the thread reader's Draft Assist panel so the
 * override UX is identical everywhere.
 */

import { ChevronDown, SlidersHorizontal } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { DraftLengthHint, VoiceRegister } from "./types";

interface ToneControlsProps {
  /** Daemon's "Matched to …" note, or null before the first draft. */
  contextNote: string | null;
  register: VoiceRegister;
  onRegisterChange: (value: VoiceRegister) => void;
  length: DraftLengthHint;
  onLengthChange: (value: DraftLengthHint) => void;
  /** True once the user has hand-set the tone (overriding inference). */
  overridden: boolean;
  onResetTone: () => void;
  /** Unique id prefix so multiple instances don't collide on label `htmlFor`. */
  idPrefix: string;
  /** Hint shown before any draft has been generated. */
  idleHint?: string;
}

export function ToneControls({
  contextNote,
  register,
  onRegisterChange,
  length,
  onLengthChange,
  overridden,
  onResetTone,
  idPrefix,
  idleHint,
}: ToneControlsProps) {
  const [adjustOpen, setAdjustOpen] = useState(false);
  return (
    <Collapsible open={adjustOpen} onOpenChange={setAdjustOpen}>
      <div className="flex items-center gap-2">
        {contextNote ? (
          <span className="inline-flex items-center gap-1 rounded-full bg-primary/15 px-2 py-0.5 text-2xs text-primary">
            {contextNote}
          </span>
        ) : (
          <span className="min-w-0 truncate text-2xs text-muted-foreground">
            {idleHint ?? "Tone auto-matched to this person"}
          </span>
        )}
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="group/adjust ml-auto flex shrink-0 items-center gap-1.5 text-2xs text-muted-foreground outline-none transition-colors hover:text-foreground"
          >
            <SlidersHorizontal className="size-3" />
            <span>{overridden ? `${register} · ${length}` : "Adjust"}</span>
            <ChevronDown className="size-3 transition-transform duration-150 group-data-[state=open]/adjust:rotate-180" />
          </button>
        </CollapsibleTrigger>
      </div>
      <CollapsibleContent>
        <div className="mt-2 flex flex-wrap items-end gap-3">
          <div>
            <Label htmlFor={`${idPrefix}-register`}>Register</Label>
            <Select
              value={register}
              onValueChange={(value) => onRegisterChange(value as VoiceRegister)}
            >
              <SelectTrigger id={`${idPrefix}-register`} className="mt-1 h-8 w-[132px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="casual">Casual</SelectItem>
                <SelectItem value="neutral">Neutral</SelectItem>
                <SelectItem value="formal">Formal</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div>
            <Label htmlFor={`${idPrefix}-length`}>Length</Label>
            <Select
              value={length}
              onValueChange={(value) => onLengthChange(value as DraftLengthHint)}
            >
              <SelectTrigger id={`${idPrefix}-length`} className="mt-1 h-8 w-[132px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="short">Short</SelectItem>
                <SelectItem value="medium">Medium</SelectItem>
                <SelectItem value="long">Long</SelectItem>
              </SelectContent>
            </Select>
          </div>
          {overridden ? (
            <Button type="button" variant="ghost" size="sm" onClick={onResetTone}>
              Reset to auto
            </Button>
          ) : null}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
