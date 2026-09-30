/*
 * Where an AI draft came from, in words: which model (local or cloud), how
 * many of your emails shaped the voice, whether your history was used, and
 * where each source opens. The CLI and TUI word the line the same way
 * (`DraftProvenanceData::summary_line` in mxr-protocol).
 */

import type { components } from "@/api/generated";
import { firstName } from "@/features/thread/context/contextFormat";

export type DraftProvenance = components["schemas"]["DraftProvenanceData"];
export type DraftSource = components["schemas"]["DraftSourceData"];
type Locality = components["schemas"]["AiLocalityData"];

function modelLabel(locality: Locality, model: string): string {
  return `${locality === "local" ? "Local" : "Cloud"} model ${model}`;
}

/** "Maya" from "Maya Chen"; the address when there's no name. */
export function shortPerson(source: DraftSource): string {
  return firstName({ display_name: source.person_name ?? null, email: source.person });
}

/** "Local model gemma4 · used 5 of your emails to Maya · history used". */
export function draftProvenanceLine(provenance: DraftProvenance): string {
  const parts = [modelLabel(provenance.locality, provenance.model)];
  const examples = provenance.voice_examples ?? [];
  const first = examples[0];
  if (first) {
    const samePerson = examples.every(
      (source) => source.person.toLowerCase() === first.person.toLowerCase(),
    );
    parts.push(
      samePerson && first.person
        ? `used ${examples.length} of your emails to ${shortPerson(first)}`
        : `used ${examples.length} of your emails`,
    );
  }
  parts.push(provenance.history_used ? "history used" : "history not used");
  const rewrite = provenance.rewrite;
  if (rewrite) {
    const by = `${rewrite.locality === "local" ? "local" : "cloud"} model ${rewrite.model}`;
    parts.push(
      rewrite.outcome === "rejected"
        ? `rewrite attempted by ${by}, not used`
        : rewrite.outcome === "skipped"
          ? `rewrite by ${by} skipped to keep your history local`
          : `rewritten by ${by}`,
    );
  }
  return parts.join(" · ");
}

/** "Your email to Maya" / "Maya" / "You", for one source in the list. */
export function draftSourceLabel(source: DraftSource, kind: "voice" | "conversation"): string {
  if (kind === "voice") return `Your email to ${shortPerson(source)}`;
  return source.from_me ? "You" : shortPerson(source);
}

/**
 * The reader route that opens the cited message: your own emails from Sent,
 * anyone else's from All Mail, with the message expanded and in view.
 */
export function draftSourceRoute(source: DraftSource) {
  return {
    to: "/m/$mailbox/$threadId",
    params: { mailbox: source.from_me ? "sent" : "archive", threadId: source.thread_id },
    search: { message: source.message_id },
  } as const;
}
