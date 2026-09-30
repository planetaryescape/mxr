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
import { DraftProvenanceLine } from "@/features/compose/DraftProvenanceLine";
import type { DraftProvenance } from "@/features/compose/draftProvenanceFormat";
import { ToneControls } from "@/features/compose/ToneControls";
import { replyWithBodyIntent, useComposeUi } from "@/features/compose/composeUiStore";
import type { DraftLengthHint, VoiceRegister } from "@/features/compose/types";
import { draftAssistThread, type DraftAssistResponse } from "@/features/mailbox/api";

interface DraftAssistPanelProps {
  threadId: string;
  /** The message a reply answers; enables "Reply with this". */
  replyToMessageId?: string;
}

function extractBody(response: DraftAssistResponse): string {
  return response.body ?? response.draft ?? response.message ?? "";
}

export function DraftAssistPanel({ threadId, replyToMessageId }: DraftAssistPanelProps) {
  const openCompose = useComposeUi((state) => state.openCompose);
  const [instruction, setInstruction] = useState("");
  const [body, setBody] = useState("");
  const [register, setRegister] = useState<VoiceRegister>("neutral");
  const [length, setLength] = useState<DraftLengthHint>("medium");
  const [overridden, setOverridden] = useState(false);
  const [contextNote, setContextNote] = useState<string | null>(null);
  const [provenance, setProvenance] = useState<DraftProvenance | null>(null);

  const generate = useMutation({
    mutationFn: () =>
      draftAssistThread({
        threadId,
        instruction,
        ...(overridden ? { register, lengthHint: length } : {}),
      }),
    onSuccess: (response) => {
      setBody(extractBody(response));
      setProvenance(response.provenance ?? null);
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
          Drafts a reply in your voice, from how you write to this person. Say what it should say,
          or leave it empty to answer what they asked.
        </p>
      </div>
      <Input
        autoFocus
        aria-label="Draft instruction"
        placeholder="What should it say? (optional)"
        value={instruction}
        onChange={(e) => setInstruction(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !generate.isPending) {
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
        <Button size="sm" disabled={generate.isPending} onClick={() => generate.mutate()}>
          {generate.isPending ? "Generating…" : "Generate"}
        </Button>
        {body && replyToMessageId ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => openCompose(replyWithBodyIntent(replyToMessageId, body), "overlay")}
          >
            Reply with this
          </Button>
        ) : null}
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
      {body ? <DraftProvenanceLine provenance={provenance} /> : null}
    </div>
  );
}
