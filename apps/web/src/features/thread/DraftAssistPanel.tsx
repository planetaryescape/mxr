/*
 * Right-rail panel for draft-assist. The user types an instruction, the bridge
 * drafts a reply (grounded on the thread + relationship), and the panel renders
 * it. Tone/length are inferred from how the user writes to this thread and
 * shown as a "Matched to …" chip; the user can override via the shared
 * ToneControls (parity with compose "Draft for me").
 */

import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ToneControls } from "@/features/compose/ToneControls";
import type { DraftLengthHint, VoiceRegister } from "@/features/compose/types";
import { draftAssistThread, type DraftAssistResponse } from "@/features/mailbox/api";

interface DraftAssistPanelProps {
  threadId: string;
}

function extractBody(response: DraftAssistResponse): string {
  return response.body ?? response.draft ?? response.message ?? "";
}

export function DraftAssistPanel({ threadId }: DraftAssistPanelProps) {
  const [instruction, setInstruction] = useState("");
  const [body, setBody] = useState("");
  const [register, setRegister] = useState<VoiceRegister>("neutral");
  const [length, setLength] = useState<DraftLengthHint>("medium");
  const [overridden, setOverridden] = useState(false);
  const [contextNote, setContextNote] = useState<string | null>(null);

  const generate = useMutation({
    mutationFn: () =>
      draftAssistThread({
        threadId,
        instruction,
        ...(overridden ? { register, lengthHint: length } : {}),
      }),
    onSuccess: (response) => {
      setBody(extractBody(response));
      if (response.context_note) setContextNote(response.context_note);
      // Reflect the inferred tone in the dials (unless the user overrode it).
      if (!overridden) {
        if (response.inferred_register) setRegister(response.inferred_register);
        if (response.inferred_length) setLength(response.inferred_length);
      }
    },
    onError: (error: Error) => toast.error("Draft-assist failed", { description: error.message }),
  });

  return (
    <div className="space-y-3">
      <div>
        <h3 className="text-sm font-semibold text-foreground">Draft assist</h3>
        <p className="text-2xs text-muted-foreground">
          Ask the LLM to draft a reply for the focused thread.
        </p>
      </div>
      <Input
        autoFocus
        aria-label="Draft instruction"
        placeholder="Ask for a polite decline, a follow-up, etc."
        value={instruction}
        onChange={(e) => setInstruction(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !generate.isPending && instruction.trim()) {
            e.preventDefault();
            generate.mutate();
          }
        }}
      />
      <ToneControls
        contextNote={contextNote}
        register={register}
        onRegisterChange={(value) => {
          setRegister(value);
          setOverridden(true);
        }}
        length={length}
        onLengthChange={(value) => {
          setLength(value);
          setOverridden(true);
        }}
        overridden={overridden}
        onResetTone={() => setOverridden(false)}
        idPrefix="thread-draft"
        idleHint="Tone will match how you write to this thread"
      />
      <div className="flex gap-2">
        <Button
          size="sm"
          disabled={generate.isPending || !instruction.trim()}
          onClick={() => generate.mutate()}
        >
          {generate.isPending ? "Generating…" : "Generate"}
        </Button>
        {body ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              navigator.clipboard
                .writeText(body)
                .then(() => toast.success("Copied to clipboard"))
                .catch((err: Error) => toast.error("Copy failed", { description: err.message }));
            }}
          >
            Copy
          </Button>
        ) : null}
      </div>
      {body ? (
        <pre
          aria-label="Draft preview"
          className="whitespace-pre-wrap rounded-md border border-border bg-muted/40 p-3 font-mono text-2xs leading-relaxed text-foreground"
        >
          {body}
        </pre>
      ) : null}
    </div>
  );
}
